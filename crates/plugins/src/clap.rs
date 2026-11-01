//! CLAP effects through clack-host: offline processing in blocks, state,
//! and the editor (embedded in our window on Windows, floating elsewhere if
//! the plugin offers it). Everything runs on the plugin thread, which is the
//! "main thread" and the "audio thread" for the plugin at once.

use std::cell::RefCell;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_extensions::timer::{HostTimer, HostTimerImpl, PluginTimer, TimerId};
use clack_host::prelude::*;

use crate::{Editor, Effect, PluginInfo};

pub(crate) struct Host;

impl HostHandlers for Host {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &Self::Shared<'_>) {
        builder.register::<HostLog>().register::<HostGui>().register::<HostState>().register::<HostTimer>();
    }
}

#[derive(Clone, Copy, Default)]
struct Exts {
    state: Option<PluginState>,
    gui: Option<PluginGui>,
    timer: Option<PluginTimer>,
    ports: Option<PluginAudioPorts>,
}

pub(crate) struct Shared {
    ext: OnceLock<Exts>,
    callback: AtomicBool,
    gui_closed: AtomicBool,
    resize: Mutex<Option<(u32, u32)>>,
    dirty: AtomicBool,
}

impl<'a> SharedHandler<'a> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {
        self.callback.store(true, Ordering::SeqCst);
    }
}

impl HostLogImpl for Shared {
    fn log(&self, severity: LogSeverity, message: &str) {
        if severity >= LogSeverity::Warning {
            log::info!("CLAP plugin: {message}");
        }
    }
}

impl HostGuiImpl for Shared {
    fn resize_hints_changed(&self) {}
    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        *self.resize.lock().unwrap_or_else(|e| e.into_inner()) = Some((new_size.width, new_size.height));
        Ok(())
    }
    fn request_show(&self) -> Result<(), HostError> {
        Ok(())
    }
    fn request_hide(&self) -> Result<(), HostError> {
        Ok(())
    }
    fn closed(&self, _was_destroyed: bool) {
        self.gui_closed.store(true, Ordering::SeqCst);
    }
}

pub(crate) struct Main<'a> {
    shared: &'a Shared,
    timers: RefCell<Vec<(TimerId, Duration, Instant)>>,
    next_timer: RefCell<u32>,
}

impl<'a> MainThreadHandler<'a> for Main<'a> {
    fn initialized(&self, instance: InitializedPluginHandle<'a>) {
        let _ = self.shared.ext.set(Exts { state: instance.get_extension(), gui: instance.get_extension(), timer: instance.get_extension(), ports: instance.get_extension() });
    }
}

impl HostStateImpl for Main<'_> {
    fn mark_dirty(&self) {
        self.shared.dirty.store(true, Ordering::SeqCst);
    }
}

impl HostTimerImpl for Main<'_> {
    fn register_timer(&self, period_ms: u32) -> Result<TimerId, HostError> {
        let mut n = self.next_timer.borrow_mut();
        *n += 1;
        let id = TimerId(*n);
        let period = Duration::from_millis(period_ms.max(5) as u64);
        self.timers.borrow_mut().push((id, period, Instant::now() + period));
        Ok(id)
    }
    fn unregister_timer(&self, timer_id: TimerId) -> Result<(), HostError> {
        self.timers.borrow_mut().retain(|(id, ..)| *id != timer_id);
        Ok(())
    }
}

/// The plugins of a .clap file.
pub fn scan_file(path: &std::path::Path) -> Result<Vec<PluginInfo>, String> {
    // SAFETY: loading a plugin library runs its code; we only load files the
    // user put into a plugin folder or picked.
    let entry = unsafe { PluginEntry::load(path) }.map_err(|e| format!("can't load it: {e}"))?;
    let factory = entry.get_plugin_factory().ok_or("it has no CLAP plugins")?;
    Ok(factory
        .plugin_descriptors()
        .filter_map(|d| {
            let id = d.id()?.to_string_lossy().into_owned();
            let is_effect = d.features().any(|f| f.to_bytes() == b"audio-effect");
            let is_instrument = d.features().any(|f| f.to_bytes() == b"instrument");
            (is_effect || !is_instrument).then(|| PluginInfo {
                format: crate::Format::Clap,
                path: path.to_string_lossy().into_owned(),
                id,
                name: d.name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                vendor: d.vendor().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            })
        })
        .collect())
}

pub struct ClapEffect {
    // field order = drop order: processor, instance, then the library
    processor: Option<StartedPluginAudioProcessor<Host>>,
    instance: PluginInstance<Host>,
    _entry: PluginEntry,
    inputs: AudioPorts,
    outputs: AudioPorts,
    events_in: EventBuffer,
    events_out: EventBuffer,
    in_buf: Vec<Vec<f32>>,
    out_buf: Vec<Vec<f32>>,
    channels: usize,
    block: usize,
    editor: Option<Editor>,
    floating: bool,
}

impl ClapEffect {
    pub fn load(path: &str, id: &str, rate: f64, block: usize) -> Result<Self, String> {
        // SAFETY: see scan_file.
        let entry = unsafe { PluginEntry::load(path) }.map_err(|e| format!("{path}: {e}"))?;
        let host = HostInfo::new("TTV-TTS", "ZcarecroW", "https://github.com/ZcarecroW/TTV-TTS", env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
        let cid = CString::new(id).map_err(|e| e.to_string())?;
        let mut instance = PluginInstance::<Host>::new(
            |_| Shared { ext: OnceLock::new(), callback: AtomicBool::new(false), gui_closed: AtomicBool::new(false), resize: Mutex::new(None), dirty: AtomicBool::new(false) },
            |shared| Main { shared, timers: RefCell::new(Vec::new()), next_timer: RefCell::new(0) },
            &entry,
            &cid,
            &host,
        )
        .map_err(|e| format!("{id}: {e}"))?;
        let exts = instance.access_shared_handler(|s| s.ext.get().copied()).unwrap_or_default();
        // the main input's channels (mono or stereo; others get stereo)
        let channels = exts
            .ports
            .and_then(|p| {
                let mut buf = AudioPortInfoBuffer::new();
                let h = instance.plugin_handle();
                (p.count(&h, true) > 0).then(|| p.get(&h, 0, true, &mut buf).map(|i| i.channel_count as usize)).flatten()
            })
            .filter(|c| (1..=2).contains(c))
            .unwrap_or(2);
        let config = PluginAudioConfiguration { sample_rate: rate, min_frames_count: 1, max_frames_count: block as u32 };
        let processor = instance.activate(|_, _| (), config).map_err(|e| format!("activating {id}: {e}"))?;
        let processor = processor.start_processing().map_err(|e| format!("starting {id}: {e}"))?;
        Ok(Self {
            processor: Some(processor),
            instance,
            _entry: entry,
            inputs: AudioPorts::with_capacity(channels, 1),
            outputs: AudioPorts::with_capacity(channels, 1),
            events_in: EventBuffer::new(),
            events_out: EventBuffer::new(),
            in_buf: vec![vec![0.0; block]; channels],
            out_buf: vec![vec![0.0; block]; channels],
            channels,
            block,
            editor: None,
            floating: false,
        })
    }

    fn exts(&mut self) -> Exts {
        self.instance.access_shared_handler(|s| s.ext.get().copied()).unwrap_or_default()
    }
}

impl Effect for ClapEffect {
    fn process(&mut self, x: &mut [f32]) {
        let Some(p) = self.processor.as_mut() else { return };
        for chunk in x.chunks_mut(self.block) {
            let n = chunk.len();
            for ch in &mut self.in_buf {
                ch[..n].copy_from_slice(chunk);
            }
            let input = self.inputs.with_input_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(self.in_buf.iter_mut().map(|b| InputChannel::variable(&mut b[..n]))),
            }]);
            let mut output = self.outputs.with_output_buffers([AudioPortBuffer { latency: 0, channels: AudioPortBufferType::f32_output_only(self.out_buf.iter_mut().map(|b| &mut b[..n])) }]);
            self.events_out.clear();
            let ok = p.process(&input, &mut output, &InputEvents::from_buffer(&self.events_in), &mut OutputEvents::from_buffer(&mut self.events_out), None, None).is_ok();
            if ok {
                // back to mono: the average of the channels
                let inv = 1.0 / self.channels as f32;
                for (i, s) in chunk.iter_mut().enumerate() {
                    *s = self.out_buf.iter().map(|c| c[i]).sum::<f32>() * inv;
                }
            }
        }
    }

    fn save_state(&mut self) -> Option<Vec<u8>> {
        let state = self.exts().state?;
        let mut out = Vec::new();
        state.save(&self.instance.plugin_handle(), &mut out).ok()?;
        Some(out)
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        let state = self.exts().state.ok_or("this plugin can't load settings")?;
        state.load(&self.instance.plugin_handle(), &mut std::io::Cursor::new(data)).map_err(|e| e.to_string())
    }

    fn has_editor(&mut self) -> bool {
        self.exts().gui.is_some()
    }

    fn open_editor(&mut self, title: &str) -> Result<(), String> {
        if self.editor.is_some() || self.floating {
            return Ok(());
        }
        let gui = self.exts().gui.ok_or("this plugin has no editor")?;
        self.instance.access_shared_handler(|s| s.gui_closed.store(false, Ordering::SeqCst));
        let h = self.instance.plugin_handle();
        let embedded = cfg!(windows) && gui.is_api_supported(&h, GuiConfiguration { api_type: GuiApiType::WIN32, is_floating: false });
        if embedded {
            gui.create(&h, GuiConfiguration { api_type: GuiApiType::WIN32, is_floating: false }).map_err(|e| e.to_string())?;
            let (w, hgt) = gui.get_size(&h).map_or((640, 400), |s| (s.width as i32, s.height as i32));
            let win = Editor::new(title, w, hgt)?;
            #[cfg(windows)]
            {
                // SAFETY: the window outlives the editor: close_editor destroys the gui first.
                let parent = unsafe { clack_extensions::gui::Window::from_win32_hwnd(win.raw()) };
                if let Err(e) = unsafe { gui.set_parent(&h, parent) } {
                    gui.destroy(&h);
                    return Err(e.to_string());
                }
            }
            let _ = gui.show(&h);
            win.show();
            self.editor = Some(win);
            return Ok(());
        }
        // elsewhere (and plugins that only float): the plugin's own window
        let api = GuiApiType::default_for_current_platform().ok_or("no editor window type for this platform")?;
        let float = GuiConfiguration { api_type: api, is_floating: true };
        if !gui.is_api_supported(&h, float) {
            return Err("this plugin's editor can't be opened on this system".into());
        }
        gui.create(&h, float).map_err(|e| e.to_string())?;
        if let Ok(t) = CString::new(title) {
            gui.suggest_title(&h, &t);
        }
        gui.show(&h).map_err(|e| e.to_string())?;
        self.floating = true;
        Ok(())
    }

    fn close_editor(&mut self) {
        if self.editor.is_none() && !self.floating {
            return;
        }
        if let Some(gui) = self.exts().gui {
            gui.destroy(&self.instance.plugin_handle());
        }
        self.editor = None;
        self.floating = false;
    }

    fn editor_open(&self) -> bool {
        self.editor.is_some() || self.floating
    }

    fn idle(&mut self) -> bool {
        // what the plugin asked the main thread for
        if self.instance.access_shared_handler(|s| s.callback.swap(false, Ordering::SeqCst)) {
            self.instance.call_on_main_thread_callback();
        }
        let exts = self.exts();
        if let Some(t) = exts.timer {
            let now = Instant::now();
            let due: Vec<TimerId> = self.instance.access_handler(|m| {
                let mut ts = m.timers.borrow_mut();
                ts.iter_mut()
                    .filter(|(_, _, next)| *next <= now)
                    .map(|(id, period, next)| {
                        *next = now + *period;
                        *id
                    })
                    .collect()
            });
            for id in due {
                t.on_timer(&self.instance.plugin_handle(), id);
            }
        }
        if let Some(win) = &self.editor
            && let Some((w, h)) = self.instance.access_shared_handler(|s| s.resize.lock().unwrap_or_else(|e| e.into_inner()).take())
        {
            win.set_client_size(w as i32, h as i32);
        }
        // the user closed the window
        let closed = self.editor.as_ref().is_some_and(Editor::close_requested) || (self.floating && self.instance.access_shared_handler(|s| s.gui_closed.load(Ordering::SeqCst)));
        if closed {
            self.close_editor();
        }
        closed
    }

    fn tail(&self) -> f32 {
        3.0
    }

    fn editor_window(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }
}

impl Drop for ClapEffect {
    fn drop(&mut self) {
        self.close_editor();
        if let Some(p) = self.processor.take() {
            let stopped = p.stop_processing();
            self.instance.deactivate(stopped);
        }
    }
}
