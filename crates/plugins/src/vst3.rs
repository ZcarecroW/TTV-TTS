//! VST3 effects through vst3-host: offline processing in blocks, state, and
//! the editor attached to our window (Windows; elsewhere vst3-host's own
//! window would need the UI thread, so the editor is Windows-only).

use crate::{Editor, Effect, PluginInfo};

pub fn scan_file(path: &std::path::Path) -> Vec<PluginInfo> {
    match vst3_host::discovery::get_plugin_info(path) {
        Ok(i) if !i.category.to_ascii_lowercase().contains("instrument") => vec![PluginInfo {
            format: crate::Format::Vst3,
            path: path.to_string_lossy().into_owned(),
            id: i.uid,
            name: i.name,
            vendor: i.vendor,
        }],
        Ok(_) => Vec::new(),
        Err(e) => {
            log::debug!("VST3 {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Every .vst3 in the standard folders.
pub fn plugin_files() -> Vec<std::path::PathBuf> {
    vst3_host::discovery::scan_directories(&vst3_host::discovery::scan_standard_paths()).unwrap_or_default()
}

pub struct Vst3Effect {
    // the editor goes before the plugin
    editor: Option<Editor>,
    plugin: vst3_host::Plugin,
    _host: vst3_host::Vst3Host,
    buffers: vst3_host::AudioBuffers,
    channels: usize,
    block: usize,
}

impl Vst3Effect {
    pub fn load(path: &str, rate: f64, block: usize) -> Result<Self, String> {
        let mut host = vst3_host::Vst3Host::builder().sample_rate(rate).block_size(block).build().map_err(|e| e.to_string())?;
        let mut plugin = host.load_plugin(path).map_err(|e| format!("{path}: {e}"))?;
        let channels = (plugin.info().audio_inputs as usize).clamp(1, 2);
        plugin.start_processing().map_err(|e| format!("starting {path}: {e}"))?;
        let buffers = vst3_host::AudioBuffers::new(channels, channels, block, rate);
        Ok(Self { editor: None, plugin, _host: host, buffers, channels, block })
    }
}

impl Effect for Vst3Effect {
    fn process(&mut self, x: &mut [f32]) {
        for chunk in x.chunks_mut(self.block) {
            let n = chunk.len();
            for ch in &mut self.buffers.inputs {
                ch[..n].copy_from_slice(chunk);
                ch[n..].fill(0.0);
            }
            if self.plugin.process_audio(&mut self.buffers).is_ok() {
                let inv = 1.0 / self.channels as f32;
                for (i, s) in chunk.iter_mut().enumerate() {
                    *s = self.buffers.outputs.iter().map(|c| c[i]).sum::<f32>() * inv;
                }
            }
        }
    }

    fn save_state(&mut self) -> Option<Vec<u8>> {
        self.plugin.save_state().ok()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        self.plugin.load_state(data).map_err(|e| e.to_string())
    }

    fn has_editor(&mut self) -> bool {
        self.plugin.has_editor()
    }

    fn open_editor(&mut self, title: &str) -> Result<(), String> {
        if self.editor.is_some() {
            return Ok(());
        }
        if !cfg!(windows) {
            return Err("VST3 editors open on Windows only for now".into());
        }
        if !self.plugin.has_editor() {
            return Err("this plugin has no editor".into());
        }
        let (w, h) = self.plugin.get_editor_size().unwrap_or((640, 400));
        let win = Editor::new(title, w, h)?;
        // SAFETY: the window is alive until close_editor detaches the editor and drops it.
        let handle = unsafe { vst3_host::plugin::WindowHandle::from_hwnd(win.raw()) };
        self.plugin.open_editor(handle).map_err(|e| e.to_string())?;
        if let Ok((w, h)) = self.plugin.get_editor_size() {
            win.set_client_size(w, h);
        }
        win.show();
        self.editor = Some(win);
        Ok(())
    }

    fn close_editor(&mut self) {
        if self.editor.is_some() {
            let _ = self.plugin.close_editor();
            self.editor = None;
        }
    }

    fn editor_open(&self) -> bool {
        self.editor.is_some()
    }

    fn idle(&mut self) -> bool {
        let _ = self.plugin.service_host_requests();
        if let Some(win) = &self.editor
            && let Some((w, h)) = self.plugin.take_editor_resize_request()
        {
            win.set_client_size(w, h);
        }
        let closed = self.editor.as_ref().is_some_and(Editor::close_requested);
        if closed {
            self.close_editor();
        }
        closed
    }

    fn editor_window(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    fn tail(&self) -> f32 {
        let s = self.plugin.tail_samples();
        // u32::MAX = "infinite" (VST3 kInfiniteTail): cap it
        if s == 0 { 0.5 } else { (s as f32 / self.buffers.sample_rate as f32).min(5.0) }
    }
}

impl Drop for Vst3Effect {
    fn drop(&mut self) {
        self.close_editor();
        let _ = self.plugin.stop_processing();
    }
}
