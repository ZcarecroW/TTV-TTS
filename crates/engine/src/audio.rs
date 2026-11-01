//! Small audio helpers: WAV decoding, loudness, FLAC backup.

use ttv_vcp::wav;

/// Mono samples and rate of a WAV file.
pub fn decode(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    let mut d = wav::StreamDecoder::new();
    let mut out = Vec::new();
    d.push(bytes, &mut out).ok()?;
    let f = d.format()?;
    if f.channels > 1 {
        let ch = f.channels as usize;
        out = out.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
    }
    Some((out, f.sample_rate))
}

/// Loudness of speech in dBFS (RMS over the parts above a noise gate).
pub fn loudness_db(samples: &[f32]) -> Option<f32> {
    let (mut sum, mut n) = (0f64, 0usize);
    for s in samples.iter().filter(|s| s.abs() > 0.01) {
        sum += (*s as f64) * (*s as f64);
        n += 1;
    }
    if n < 1000 {
        return None;
    }
    Some(10.0 * (sum / n as f64).log10() as f32)
}

/// Linear gain that brings speech to the target loudness (−20 dBFS RMS),
/// limited so quiet takes aren't pushed into clipping.
#[cfg(test)]
pub fn gain_for(samples: &[f32]) -> f32 {
    loudness_db(samples).map_or(1.0, gain_from_db)
}

/// Gain for speech measured at `db` dBFS.
pub fn gain_from_db(db: f32) -> f32 {
    const TARGET: f32 = -20.0;
    10f32.powf((TARGET - db) / 20.0).clamp(0.4, 2.5)
}

/// Lossless FLAC of mono 16-bit samples.
pub fn flac(samples: &[f32], rate: u32) -> Option<Vec<u8>> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let ints: Vec<i32> = samples.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i32).collect();
    let config = flacenc::config::Encoder::default().into_verified().ok()?;
    let source = flacenc::source::MemSource::from_samples(&ints, 1, 16, rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).ok()?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).ok()?;
    Some(sink.as_slice().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_and_gain() {
        let quiet: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.05).sin() * 0.05).collect();
        let loud: Vec<f32> = quiet.iter().map(|s| s * 8.0).collect();
        assert!(gain_for(&quiet) > 1.5);
        assert!(gain_for(&loud) < 1.0);
        assert_eq!(gain_for(&[0.0; 100]), 1.0);
    }

    #[test]
    fn flac_is_smaller_than_pcm() {
        let s: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.03).sin() * 0.3).collect();
        let f = flac(&s, 24_000).expect("flac");
        assert!(f.starts_with(b"fLaC"));
        assert!(f.len() < s.len() * 2, "{} bytes", f.len());
    }
}
