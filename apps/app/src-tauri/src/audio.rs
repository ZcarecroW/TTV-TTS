//! Audio output to a device of the user's choice (cpal). Clips (mono, any
//! sample rate) are resampled once to the device rate, then queued for the
//! realtime callback, which plays them back to back without gaps. The
//! callback never locks or allocates: clips arrive through a bounded channel,
//! volume and skip requests through atomics.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::time::Duration;

use audioadapter_buffers::direct::InterleavedSlice;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{Fft, FixedSync, Resampler};
use serde::Serialize;

/// Fade-out length when a clip is skipped (avoids clicks).
const FADE_SECS: f32 = 0.008;

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    /// Stable id (cpal `DeviceId`), stored in the settings.
    pub id: String,
    pub name: String,
    pub default: bool,
}

struct Clip {
    id: u64,
    samples: Arc<[f32]>,
    gain: f32,
}

/// Shared between the control side and the realtime callback.
struct Shared {
    volume: AtomicU32,
    /// Clips with an id up to this one are skipped (faded out).
    skip_upto: AtomicU64,
    /// Highest clip id that finished playing (or was skipped).
    finished: AtomicU64,
    /// The stream reported an error (device gone): reopen it.
    broken: AtomicBool,
    /// Device sample rate of the open stream.
    rate: AtomicU32,
}

enum Cmd {
    Open(Option<String>),
    Play(u64, Vec<f32>, u32, f32),
}

pub struct Audio {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
    /// For callers that resample themselves (the plugin stage).
    resamplers: parking_lot::Mutex<HashMap<(u32, u32), Fft<f32>>>,
}

impl Audio {
    pub fn start(device: Option<String>, volume: f32) -> Self {
        let shared = Arc::new(Shared {
            volume: AtomicU32::new(volume.to_bits()),
            skip_upto: AtomicU64::new(0),
            finished: AtomicU64::new(0),
            broken: AtomicBool::new(false),
            rate: AtomicU32::new(48_000),
        });
        let (tx, rx) = std::sync::mpsc::channel();
        let sh = shared.clone();
        std::thread::Builder::new()
            .name("ttv-audio".into())
            .spawn(move || audio_thread(rx, sh, device))
            .expect("audio thread");
        Self { tx, shared, next_id: AtomicU64::new(1), resamplers: parking_lot::Mutex::new(HashMap::new()) }
    }

    /// Switches to another output device (None = system default).
    pub fn open(&self, device: Option<String>) {
        let _ = self.tx.send(Cmd::Open(device));
    }

    /// The output device's sample rate.
    pub fn rate(&self) -> u32 {
        self.shared.rate.load(Ordering::Relaxed)
    }

    /// Mono samples from one rate to another.
    pub fn resample(&self, samples: Vec<f32>, from: u32, to: u32) -> Vec<f32> {
        resample(&mut self.resamplers.lock(), samples, from, to)
    }

    pub fn set_volume(&self, v: f32) {
        self.shared.volume.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Queues a mono clip; returns its id. Plays right after earlier clips.
    pub fn play(&self, samples: Vec<f32>, rate: u32, gain: f32) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Cmd::Play(id, samples, rate, gain));
        id
    }

    /// Stops everything queued up to now (with a short fade).
    pub fn skip_all(&self) {
        let last = self.next_id.load(Ordering::Relaxed).saturating_sub(1);
        self.shared.skip_upto.fetch_max(last, Ordering::Relaxed);
    }

    pub fn is_finished(&self, id: u64) -> bool {
        self.shared.finished.load(Ordering::Relaxed) >= id
    }

    /// Waits until clip `id` has played (or was skipped).
    pub async fn wait(&self, id: u64) {
        while !self.is_finished(id) {
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }
}

pub fn devices() -> Vec<OutputDevice> {
    let host = cpal::default_host();
    let default_id = host.default_output_device().and_then(|d| d.id().ok()).map(|i| i.to_string());
    let mut out = Vec::new();
    if let Ok(list) = host.output_devices() {
        for d in list {
            let Ok(id) = d.id() else { continue };
            let id = id.to_string();
            out.push(OutputDevice { default: default_id.as_deref() == Some(id.as_str()), name: d.to_string(), id });
        }
    }
    out
}

fn find_device(host: &cpal::Host, id: Option<&str>) -> Option<cpal::Device> {
    if let Some(want) = id
        && let Ok(list) = host.output_devices()
    {
        for d in list {
            if d.id().map(|i| i.to_string()).as_deref() == Ok(want) {
                return Some(d);
            }
        }
        log::warn!("audio device {want} not found - using the default output");
    }
    host.default_output_device()
}

fn audio_thread(rx: Receiver<Cmd>, shared: Arc<Shared>, mut device_id: Option<String>) {
    let mut resamplers: HashMap<(u32, u32), Fft<f32>> = HashMap::new();
    let mut stream: Option<(cpal::Stream, SyncSender<Clip>)> = None;
    let mut want_open = true;
    loop {
        if want_open || shared.broken.swap(false, Ordering::Relaxed) {
            want_open = false;
            stream = None;
            match open_stream(device_id.as_deref(), shared.clone()) {
                Ok(s) => stream = Some(s),
                Err(e) => log::error!("audio output: {e}"),
            }
        }
        let cmd = match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(c) => c,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return,
        };
        match cmd {
            Cmd::Open(id) => {
                device_id = id;
                want_open = true;
            }
            Cmd::Play(id, samples, rate, gain) => {
                let out_rate = shared.rate.load(Ordering::Relaxed);
                let samples = resample(&mut resamplers, samples, rate, out_rate);
                match &stream {
                    Some((_, tx)) => {
                        // Bounded: blocks only if dozens of clips are queued.
                        if tx.send(Clip { id, samples: samples.into(), gain }).is_err() {
                            shared.finished.fetch_max(id, Ordering::Relaxed);
                        }
                    }
                    None => {
                        shared.finished.fetch_max(id, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

fn resample(cache: &mut HashMap<(u32, u32), Fft<f32>>, input: Vec<f32>, from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 {
        return input;
    }
    let r = match cache.entry((from, to)) {
        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
        std::collections::hash_map::Entry::Vacant(v) => match Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Input) {
            Ok(r) => v.insert(r),
            Err(e) => {
                log::warn!("resampler {from}->{to}: {e}");
                return input;
            }
        },
    };
    let frames = input.len();
    let mut out = vec![0f32; r.process_all_needed_output_len(frames)];
    let out_len = out.len();
    let Ok(inp) = InterleavedSlice::new(&input, 1, frames) else { return input };
    let Ok(mut outp) = InterleavedSlice::new_mut(&mut out, 1, out_len) else { return input };
    match r.process_all_into_buffer(&inp, &mut outp, frames, None) {
        Ok((_, written)) => {
            out.truncate(written);
            out
        }
        Err(e) => {
            log::warn!("resampling failed: {e}");
            input
        }
    }
}

fn open_stream(device_id: Option<&str>, shared: Arc<Shared>) -> Result<(cpal::Stream, SyncSender<Clip>), String> {
    let host = cpal::default_host();
    let device = find_device(&host, device_id).ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    shared.rate.store(config.sample_rate, Ordering::Relaxed);
    let (tx, rx) = sync_channel::<Clip>(64);
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, config, rx, shared),
        cpal::SampleFormat::I16 => build::<i16>(&device, config, rx, shared),
        cpal::SampleFormat::U16 => build::<u16>(&device, config, rx, shared),
        cpal::SampleFormat::I32 => build::<i32>(&device, config, rx, shared),
        cpal::SampleFormat::F64 => build::<f64>(&device, config, rx, shared),
        other => return Err(format!("unsupported sample format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    log::info!("audio output: {device} ({} Hz, {} ch, {format:?})", config.sample_rate, config.channels);
    Ok((stream, tx))
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, rx: Receiver<Clip>, shared: Arc<Shared>) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let fade_len = ((config.sample_rate as f32 * FADE_SECS) as usize).max(1);
    let mut current: Option<(Clip, usize)> = None;
    let mut fading: Option<usize> = None;
    let err_shared = shared.clone();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _info| {
                let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let skip = shared.skip_upto.load(Ordering::Relaxed);
                for frame in out.chunks_mut(channels) {
                    let mut v = 0.0f32;
                    loop {
                        if current.is_none() {
                            match rx.try_recv() {
                                Ok(c) => {
                                    if c.id <= skip {
                                        shared.finished.fetch_max(c.id, Ordering::Relaxed);
                                        continue;
                                    }
                                    current = Some((c, 0));
                                    fading = None;
                                }
                                Err(_) => break,
                            }
                        }
                        let (clip, pos) = current.as_mut().expect("clip");
                        if clip.id <= skip && fading.is_none() {
                            fading = Some(fade_len);
                        }
                        let ended = *pos >= clip.samples.len() || fading == Some(0);
                        if ended {
                            shared.finished.fetch_max(clip.id, Ordering::Relaxed);
                            current = None;
                            continue;
                        }
                        let mut s = clip.samples[*pos] * clip.gain;
                        if let Some(f) = fading.as_mut() {
                            s *= *f as f32 / fade_len as f32;
                            *f -= 1;
                        }
                        *pos += 1;
                        v = s;
                        break;
                    }
                    let sample = T::from_sample((v * volume).clamp(-1.0, 1.0));
                    for ch in frame.iter_mut() {
                        *ch = sample;
                    }
                }
            },
            move |e| {
                log::warn!("audio stream error: {e}");
                err_shared.broken.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_24k_to_48k_and_44k1() {
        let mut cache = HashMap::new();
        let sine: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.5).collect();
        let up = resample(&mut cache, sine.clone(), 24_000, 48_000);
        assert!((up.len() as i64 - 48_000).abs() < 64, "48k length {}", up.len());
        let cd = resample(&mut cache, sine, 24_000, 44_100);
        assert!((cd.len() as i64 - 44_100).abs() < 64, "44.1k length {}", cd.len());
        let peak = cd.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.45 && peak < 0.55, "peak {peak}");
    }
}
