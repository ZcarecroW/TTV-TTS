//! Clip resampling (FFT resampler, one per rate pair, kept): plugins run at
//! a fixed rate, speech arrives at the voice's rate.

use std::collections::HashMap;

use audioadapter_buffers::direct::InterleavedSlice;
use parking_lot::Mutex;
use rubato::{Fft, FixedSync, Resampler};

#[derive(Default)]
pub struct Resamplers(Mutex<HashMap<(u32, u32), Fft<f32>>>);

impl Resamplers {
    /// A whole mono clip from rate `from` to `to` (unchanged on errors).
    pub fn resample(&self, input: Vec<f32>, from: u32, to: u32) -> Vec<f32> {
        if from == to || input.is_empty() || from == 0 || to == 0 {
            return input;
        }
        let mut cache = self.0.lock();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_the_rate_and_keeps_the_level() {
        let r = Resamplers::default();
        let x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.5).collect();
        let y = r.resample(x, 24_000, 48_000);
        assert!((y.len() as i64 - 48_000).abs() < 64, "{}", y.len());
        let peak = y[1000..47_000].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.02, "{peak}");
        assert_eq!(r.resample(vec![0.1; 10], 48_000, 48_000), vec![0.1; 10]);
    }
}
