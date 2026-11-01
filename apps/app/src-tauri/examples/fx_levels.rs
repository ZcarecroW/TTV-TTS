//! Measures how loud every voice effect plays compared with the dry voice,
//! on real voice samples (a folder of WAV files), and how long it takes.
//!   cargo run --release -p ttv-tts --example fx_levels -- <dir with .wav> [--out <dir>]
//! `--out` also writes each effect applied to the first sample (to listen).

use ttv_tts_lib::fx::{ALL, apply};

fn loud(x: &[f32]) -> Option<f32> {
    let (mut sum, mut n) = (0f64, 0usize);
    for s in x.iter().filter(|s| s.abs() > 0.01) {
        sum += (*s as f64).powi(2);
        n += 1;
    }
    (n >= 1000).then(|| 10.0 * (sum / n as f64).log10() as f32)
}

fn decode(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    let mut d = ttv_vcp::wav::StreamDecoder::new();
    let mut out = Vec::new();
    d.push(bytes, &mut out).ok()?;
    let f = d.format()?;
    if f.channels > 1 {
        let ch = f.channels as usize;
        out = out.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
    }
    Some((out, f.sample_rate))
}

fn write_wav(path: &std::path::Path, x: &[f32], rate: u32) {
    let mut b = Vec::with_capacity(44 + x.len() * 2);
    let data = (x.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in x {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().expect("folder with .wav files");
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let mut clips = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(dir).expect("dir").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
    names.sort();
    for p in names.iter().take(60) {
        if let Some((mut x, rate)) = std::fs::read(p).ok().and_then(|b| decode(&b))
            && let Some(l) = loud(&x)
        {
            // like the app: every voice is brought to -20 dBFS first
            let g = 10f32.powf((-20.0 - l) / 20.0);
            x.iter_mut().for_each(|s| *s *= g);
            let l = loud(&x).unwrap_or(-20.0);
            clips.push((x, rate, l));
        }
    }
    println!("{} clips, {:.0} s of speech", clips.len(), clips.iter().map(|c| c.0.len() as f32 / c.1 as f32).sum::<f32>());
    println!("{:<11} {:>9} {:>9} {:>8} {:>10}", "effect", "median Δ", "spread", "makeup", "ms per min");
    for f in ALL {
        let mut deltas = Vec::new();
        let mut secs = 0f32;
        let mut audio = 0f32;
        for (x, rate, l) in &clips {
            let t = std::time::Instant::now();
            let w = apply(f, x.clone(), *rate, 1.0);
            secs += t.elapsed().as_secs_f32();
            audio += x.len() as f32 / *rate as f32;
            if let Some(lw) = loud(&w) {
                deltas.push(lw - l);
            }
        }
        deltas.sort_by(f32::total_cmp);
        let med = deltas[deltas.len() / 2];
        let spread = deltas[deltas.len() * 9 / 10] - deltas[deltas.len() / 10];
        println!("{:<11} {:>+8.1} {:>9.1} {:>+8.1} {:>10.1}", f.name(), med, spread, f.makeup_db() - med, secs / audio * 60_000.0);
        if let (Some(o), Some((x, rate, _))) = (&out, clips.first()) {
            std::fs::create_dir_all(o).ok();
            write_wav(&o.join(format!("{}.wav", f.name())), &apply(f, x.clone(), *rate, 1.0), *rate);
        }
    }
    if let (Some(o), Some((x, rate, _))) = (&out, clips.first()) {
        write_wav(&o.join("dry.wav"), x, *rate);
    }
}
