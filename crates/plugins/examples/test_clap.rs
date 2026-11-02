//! A tiny CLAP effect to test the host with (built by `cargo test` as
//! target/<profile>/examples/test_clap.dll): a gain of 0.5 kept in its
//! settings, and on Windows an editor that puts one child window into the
//! host's window.

use std::cell::Cell;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};

use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts, PluginAudioPortsImpl};
use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui, PluginGuiImpl, Window};
use clack_extensions::state::{PluginState, PluginStateImpl};
use clack_plugin::plugin::features::AUDIO_EFFECT;
use clack_plugin::prelude::*;
use clack_plugin::process::audio::ChannelPair;
use clack_plugin::stream::{InputStream, OutputStream};

pub struct TestGain;

impl Plugin for TestGain {
    type AudioProcessor<'a> = Processor<'a>;
    type Shared<'a> = Shared;
    type MainThread<'a> = Main<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder.register::<PluginAudioPorts>().register::<PluginState>().register::<PluginGui>();
    }
}

impl DefaultPluginFactory for TestGain {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("io.github.zcarecrow.ttvtts.testgain", "TTV Test Gain").with_vendor("TTV-TTS tests").with_features([AUDIO_EFFECT])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared { gain: AtomicU32::new(0.5f32.to_bits()) })
    }

    fn new_main_thread<'a>(_host: HostMainThreadHandle<'a>, shared: &'a Shared) -> Result<Main<'a>, PluginError> {
        Ok(Main { shared, child: Cell::new(0) })
    }
}

pub struct Shared {
    gain: AtomicU32,
}

impl PluginShared<'_> for Shared {}

pub struct Main<'a> {
    shared: &'a Shared,
    /// The editor's child window (HWND as an integer, 0 = none; Windows only).
    #[cfg_attr(not(windows), allow(dead_code))]
    child: Cell<isize>,
}

impl<'a> PluginMainThread<'a, Shared> for Main<'a> {}

pub struct Processor<'a> {
    shared: &'a Shared,
}

impl<'a> PluginAudioProcessor<'a, Shared, Main<'a>> for Processor<'a> {
    fn activate(_host: HostAudioProcessorHandle<'a>, _main: &Main<'a>, shared: &'a Shared, _config: PluginAudioConfiguration) -> Result<Self, PluginError> {
        Ok(Self { shared })
    }

    fn process(&mut self, _process: Process, mut audio: Audio, _events: Events) -> Result<ProcessStatus, PluginError> {
        let g = f32::from_bits(self.shared.gain.load(Ordering::Relaxed));
        for mut pair in audio.port_pairs() {
            let Some(channels) = pair.channels()?.into_f32() else { continue };
            for c in channels {
                match c {
                    ChannelPair::InputOutput(i, o) => i.iter().zip(o.iter_mut()).for_each(|(x, y)| *y = x * g),
                    ChannelPair::InPlace(b) => b.iter_mut().for_each(|x| *x *= g),
                    ChannelPair::OutputOnly(o) => o.fill(0.0),
                    ChannelPair::InputOnly(_) => {}
                }
            }
        }
        Ok(ProcessStatus::ContinueIfNotQuiet)
    }
}

impl PluginAudioPortsImpl for Main<'_> {
    fn count(&self, _is_input: bool) -> u32 {
        1
    }

    fn get(&self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo { id: ClapId::new(0), name: b"main", channel_count: 2, flags: AudioPortFlags::IS_MAIN, port_type: Some(AudioPortType::STEREO), in_place_pair: None });
        }
    }
}

impl PluginStateImpl for Main<'_> {
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        output.write_all(&self.shared.gain.load(Ordering::Relaxed).to_le_bytes())?;
        Ok(())
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        let mut b = [0u8; 4];
        input.read_exact(&mut b)?;
        self.shared.gain.store(u32::from_le_bytes(b), Ordering::Relaxed);
        Ok(())
    }
}

impl PluginGuiImpl for Main<'_> {
    fn is_api_supported(&self, configuration: GuiConfiguration) -> bool {
        configuration.api_type == GuiApiType::WIN32 && !configuration.is_floating
    }
    fn get_preferred_api(&self) -> Option<GuiConfiguration<'_>> {
        Some(GuiConfiguration { api_type: GuiApiType::WIN32, is_floating: false })
    }
    fn create(&self, _configuration: GuiConfiguration) -> Result<(), PluginError> {
        Ok(())
    }
    fn destroy(&self) {
        #[cfg(windows)]
        {
            let hwnd = self.child.replace(0);
            if hwnd != 0 {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(windows::Win32::Foundation::HWND(hwnd as *mut _));
                }
            }
        }
    }
    fn set_scale(&self, _scale: f64) -> Result<(), PluginError> {
        Ok(())
    }
    fn get_size(&self) -> Option<GuiSize> {
        Some(GuiSize { width: 320, height: 120 })
    }
    fn set_size(&self, _size: GuiSize) -> Result<(), PluginError> {
        Ok(())
    }
    fn set_parent(&self, window: Window) -> Result<(), PluginError> {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WINDOW_EX_STYLE, WS_CHILD, WS_VISIBLE};
            use windows::core::w;
            let parent = window.as_win32_hwnd().ok_or(PluginError::Message("not a win32 window"))?;
            let child = unsafe {
                CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("TTV Test Gain: gain 0.5"), WS_CHILD | WS_VISIBLE, 10, 10, 300, 100, Some(windows::Win32::Foundation::HWND(parent)), None, None, None)
            }
            .map_err(|_| PluginError::Message("no child window"))?;
            self.child.set(child.0 as isize);
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            Err(PluginError::Message("editor only on Windows"))
        }
    }
    fn set_transient(&self, _window: Window) -> Result<(), PluginError> {
        Err(PluginError::Message("not floating"))
    }
    fn show(&self) -> Result<(), PluginError> {
        Ok(())
    }
    fn hide(&self) -> Result<(), PluginError> {
        Ok(())
    }
}

clack_export_entry!(SinglePluginEntry::<TestGain>);
