//! Progressive WAV reading for VCP's streaming sessions.
//!
//! VCP streams PCM16 WAV while it is still generating; the header declares a
//! placeholder length (~2 GB), so the stream has to be parsed chunk by chunk
//! instead of trusting the sizes. [`StreamDecoder`] takes arbitrary byte
//! chunks and yields whole sample frames as `f32`.

/// Format from the `fmt ` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WavError {
    #[error("not a WAV stream")]
    NotWav,
    #[error("WAV without fmt chunk")]
    NoFormat,
    #[error("only 16-bit PCM is supported (stream has {0} bit)")]
    Unsupported(u16),
    #[error("WAV header not found in the first 8 KB")]
    HeaderTooLong,
}

/// Parses the header. `Ok(None)` = more bytes needed. Returns the format and
/// the offset where the sample data starts.
pub fn parse_header(bytes: &[u8]) -> Result<Option<(Format, usize)>, WavError> {
    if bytes.len() < 12 {
        return Ok(None);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let mut off = 12usize;
    let mut fmt = None;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes([bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]]) as usize;
        if id == b"fmt " {
            if off + 24 > bytes.len() {
                return Ok(None);
            }
            let channels = u16::from_le_bytes([bytes[off + 10], bytes[off + 11]]);
            let sample_rate = u32::from_le_bytes([bytes[off + 12], bytes[off + 13], bytes[off + 14], bytes[off + 15]]);
            let bits = u16::from_le_bytes([bytes[off + 22], bytes[off + 23]]);
            fmt = Some(Format { channels: channels.max(1), sample_rate, bits });
        } else if id == b"data" {
            let f = fmt.ok_or(WavError::NoFormat)?;
            if f.bits != 16 {
                return Err(WavError::Unsupported(f.bits));
            }
            return Ok(Some((f, off + 8)));
        }
        off += 8 + size + (size & 1);
    }
    if bytes.len() > 8192 {
        return Err(WavError::HeaderTooLong);
    }
    Ok(None)
}

/// Turns a chunked WAV byte stream into interleaved `f32` samples.
#[derive(Default)]
pub struct StreamDecoder {
    head: Vec<u8>,
    format: Option<Format>,
    /// Bytes of an incomplete frame carried over to the next chunk.
    carry: Vec<u8>,
}

impl StreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn format(&self) -> Option<Format> {
        self.format
    }

    /// Feeds one chunk; appends decoded samples (interleaved) to `out`.
    pub fn push(&mut self, chunk: &[u8], out: &mut Vec<f32>) -> Result<(), WavError> {
        let data: &[u8];
        let owned: Vec<u8>;
        if self.format.is_none() {
            self.head.extend_from_slice(chunk);
            match parse_header(&self.head)? {
                None => return Ok(()),
                Some((f, start)) => {
                    self.format = Some(f);
                    owned = self.head.split_off(start);
                    self.head = Vec::new();
                    data = &owned;
                }
            }
        } else {
            data = chunk;
        }
        let f = self.format.expect("format known");
        let frame = 2 * f.channels as usize;
        // Join a carried partial frame with the start of this chunk.
        let mut rest = data;
        if !self.carry.is_empty() {
            let need = frame - self.carry.len();
            if rest.len() < need {
                self.carry.extend_from_slice(rest);
                return Ok(());
            }
            self.carry.extend_from_slice(&rest[..need]);
            rest = &rest[need..];
            let carry = std::mem::take(&mut self.carry);
            decode_pcm16(&carry, out);
        }
        let usable = rest.len() - rest.len() % frame;
        decode_pcm16(&rest[..usable], out);
        self.carry.extend_from_slice(&rest[usable..]);
        Ok(())
    }
}

#[inline]
fn decode_pcm16(bytes: &[u8], out: &mut Vec<f32>) {
    out.reserve(bytes.len() / 2);
    out.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0));
}

/// Duration of a complete WAV file in seconds (uses the real byte count).
pub fn duration_secs(wav: &[u8]) -> Option<f32> {
    let (f, start) = parse_header(wav).ok()??;
    let bytes_per_sec = f.sample_rate as f32 * f.channels as f32 * 2.0;
    Some((wav.len().saturating_sub(start)) as f32 / bytes_per_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0x7fff_fff0u32.to_le_bytes()); // placeholder like VCP
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        v.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        v.extend_from_slice(&(channels * 2).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&0x7fff_ffccu32.to_le_bytes());
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    #[test]
    fn decodes_in_odd_chunks() {
        let samples: Vec<i16> = (0..1000).map(|i| (i * 37 % 65536 - 32768) as i16).collect();
        let bytes = wav(&samples, 24000, 1);
        for chunk in [1usize, 3, 7, 44, 45, 333] {
            let mut d = StreamDecoder::new();
            let mut out = Vec::new();
            for c in bytes.chunks(chunk) {
                d.push(c, &mut out).unwrap();
            }
            assert_eq!(d.format().unwrap().sample_rate, 24000);
            assert_eq!(out.len(), samples.len(), "chunk size {chunk}");
            for (a, b) in out.iter().zip(&samples) {
                assert_eq!(*a, *b as f32 / 32768.0);
            }
        }
    }

    #[test]
    fn stereo_frames_stay_whole() {
        let samples: Vec<i16> = (0..200).collect();
        let bytes = wav(&samples, 48000, 2);
        let mut d = StreamDecoder::new();
        let mut out = Vec::new();
        for c in bytes.chunks(5) {
            d.push(c, &mut out).unwrap();
            assert_eq!(out.len() % 2, 0);
        }
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn rejects_non_wav() {
        let mut d = StreamDecoder::new();
        assert_eq!(d.push(b"{\"detail\":\"no\"}", &mut Vec::new()), Err(WavError::NotWav));
    }

    #[test]
    fn duration() {
        let bytes = wav(&vec![0i16; 24000], 24000, 1);
        assert!((duration_secs(&bytes).unwrap() - 1.0).abs() < 1e-6);
    }
}
