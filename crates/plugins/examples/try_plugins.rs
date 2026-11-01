//! Loads real plugin files, runs speech-like audio through them, saves and
//! restores their settings and opens their editors for a moment.
//!   cargo run --release -p ttv-plugins --example try_plugins -- <file.clap|file.vst3> …

use std::time::{Duration, Instant};

use ttv_plugins::{Rack, SlotSpec};

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let rack = Rack::start(|id, state| println!("  (settings of {id} kept: {} bytes)", state.len()));
    let rate = 48_000;
    // 2 s of a gliding tone with harmonics, in syllables
    let input: Vec<f32> = (0..rate * 2)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let f0 = 140.0 + 30.0 * (t * 2.0).sin();
            let s: f32 = (1..8).map(|h| (std::f32::consts::TAU * f0 * h as f32 * t).sin() / h as f32).sum();
            s * 0.2 * (t * 4.0 * std::f32::consts::PI).sin().max(0.0)
        })
        .collect();
    for f in &files {
        let t0 = Instant::now();
        let found = ttv_plugins::scan_file(std::path::Path::new(f)).unwrap_or_else(|e| {
            println!("{f}: {e}");
            Vec::new()
        });
        println!("{f}: {} effect(s) in {} ms", found.len(), t0.elapsed().as_millis());
        let Some(info) = found.into_iter().next() else { continue };
        println!("  {:?} {} by {} ({})", info.format, info.name, info.vendor, info.id);
        let id = format!("{:?}", info.format);
        let t1 = Instant::now();
        let r = rack.set(rate as u32, vec![SlotSpec { id: id.clone(), plugin: info.clone(), enabled: true, state: None }]).await;
        println!("  loaded: {:?} in {} ms", r[0].1, t1.elapsed().as_millis());
        if r[0].1.is_err() {
            continue;
        }
        let t2 = Instant::now();
        let out = rack.process(input.clone()).await;
        let took = t2.elapsed();
        let diff = input.iter().zip(&out).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        println!(
            "  processed 2 s in {:.1} ms ({:.2} % of real time): rms in {:.4} out {:.4}, max difference {:.4}, finite {}",
            took.as_secs_f32() * 1000.0,
            took.as_secs_f32() / 2.0 * 100.0,
            rms(&input),
            rms(&out),
            diff,
            out.iter().all(|s| s.is_finite())
        );
        let tail = rack.tail().await;
        println!("  tail: {:.2} s", tail.len() as f32 / rate as f32);
        let states = rack.save_states().await;
        let state = states.into_iter().find(|(i, _)| *i == id).map(|(_, s)| s);
        println!("  state: {:?} bytes", state.as_ref().map(Vec::len));
        // the same plugin again, from the saved settings
        let r2 = rack.set(rate as u32, vec![SlotSpec { id: format!("{id}-2"), plugin: info.clone(), enabled: true, state: state.clone() }]).await;
        println!("  reloaded with settings: {:?}", r2[0].1);
        let open = rack.open_editor(&format!("{id}-2")).await;
        println!("  editor: {open:?}");
        if open.is_ok() {
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            println!("  editor child windows: {}", rack.editor_children(&format!("{id}-2")).await);
            rack.close_editor(&format!("{id}-2"));
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        rack.set(rate as u32, Vec::new()).await;
    }
}
