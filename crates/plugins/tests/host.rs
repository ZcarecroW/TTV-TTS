//! The host against the two test plugins this crate builds from source
//! (examples test_clap and test_vst3; `cargo test` builds them first).

use std::path::PathBuf;
use std::time::Duration;

use ttv_plugins::{Format, Rack, SlotSpec};

/// target/<profile>/examples/<name>.dll copied to a temp file with the
/// plugin extension.
fn plugin(name: &str, ext: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap().join("examples");
    let lib = if cfg!(windows) { format!("{name}.dll") } else if cfg!(target_os = "macos") { format!("lib{name}.dylib") } else { format!("lib{name}.so") };
    let to = std::env::temp_dir().join(format!("ttv-plugin-test-{}-{name}.{ext}", std::process::id()));
    std::fs::copy(dir.join(&lib), &to).unwrap_or_else(|e| panic!("{lib}: {e} (build the examples)"));
    to
}

fn sine(rate: u32, secs: f32, amp: f32) -> Vec<f32> {
    (0..(rate as f32 * secs) as usize).map(|i| (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin() * amp).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0f32, |m, s| m.max(s.abs()))
}

#[tokio::test(flavor = "multi_thread")]
async fn clap_effect_processes_keeps_settings_and_shows_its_editor() {
    let path = plugin("test_clap", "clap");
    let found = ttv_plugins::scan_file(&path);
    assert_eq!(found.len(), 1, "{found:?}");
    let info = found[0].clone();
    assert_eq!((info.format, info.name.as_str(), info.vendor.as_str()), (Format::Clap, "TTV Test Gain", "TTV-TTS tests"));

    let rack = Rack::start(|_, _| {});
    assert!(rack.is_idle());
    let r = rack.set(48_000, vec![SlotSpec { id: "a".into(), plugin: info.clone(), enabled: true, state: None }]).await;
    assert!(r[0].1.is_ok(), "{r:?}");
    assert!(!rack.is_idle());
    // gain 0.5
    let out = rack.process(sine(48_000, 0.5, 0.8)).await;
    assert_eq!(out.len(), 24_000);
    assert!((peak(&out) - 0.4).abs() < 0.01, "peak {}", peak(&out));
    // settings: saved as the plugin keeps them, restored into a new instance
    let states = rack.save_states().await;
    assert_eq!(states, vec![("a".to_string(), 0.5f32.to_bits().to_le_bytes().to_vec())]);
    let r = rack.set(48_000, vec![SlotSpec { id: "b".into(), plugin: info.clone(), enabled: true, state: Some(0.25f32.to_bits().to_le_bytes().to_vec()) }]).await;
    assert!(r[0].1.is_ok());
    let out = rack.process(sine(48_000, 0.1, 0.8)).await;
    assert!((peak(&out) - 0.2).abs() < 0.01, "restored gain: peak {}", peak(&out));
    // switched off: the audio passes untouched
    rack.set(48_000, vec![SlotSpec { id: "b".into(), plugin: info.clone(), enabled: false, state: None }]).await;
    assert!(rack.is_idle());
    let x = sine(48_000, 0.1, 0.8);
    assert_eq!(rack.process(x.clone()).await, x);

    // the editor puts its window into ours (Windows)
    rack.set(48_000, vec![SlotSpec { id: "b".into(), plugin: info, enabled: true, state: None }]).await;
    let open = rack.open_editor("b").await;
    if ttv_plugins::editors_supported() {
        assert!(open.is_ok(), "{open:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(rack.editor_children("b").await >= 1, "the plugin drew into the editor window");
        rack.close_editor("b");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(rack.editor_children("b").await, 0);
    }
    rack.set(48_000, Vec::new()).await;
    let _ = std::fs::remove_file(path);
}

#[tokio::test(flavor = "multi_thread")]
async fn vst3_effect_processes_and_keeps_settings() {
    let path = plugin("test_vst3", "vst3");
    let found = ttv_plugins::scan_file(&path);
    assert_eq!(found.len(), 1, "{found:?}");
    let info = found[0].clone();
    assert_eq!(info.format, Format::Vst3);
    let rack = Rack::start(|_, _| {});
    let r = rack.set(48_000, vec![SlotSpec { id: "v".into(), plugin: info.clone(), enabled: true, state: None }]).await;
    assert!(r[0].1.is_ok(), "{r:?}");
    let x = sine(48_000, 0.5, 0.5);
    let out = rack.process(x.clone()).await;
    assert_eq!(out.len(), x.len());
    assert!(out.iter().all(|s| s.is_finite()));
    assert!(peak(&out) > 0.0, "the gain example passes audio");
    let states = rack.save_states().await;
    assert_eq!(states.len(), 1);
    let state = states[0].1.clone();
    let r = rack.set(48_000, vec![SlotSpec { id: "w".into(), plugin: info, enabled: true, state: Some(state) }]).await;
    assert!(r[0].1.is_ok(), "{r:?}");
    let out2 = rack.process(x.clone()).await;
    assert!((peak(&out2) - peak(&out)).abs() < 1e-3, "same settings, same sound");
    // both formats in one chain, and the ring-out
    rack.set(48_000, Vec::new()).await;
    let _ = std::fs::remove_file(path);
}
