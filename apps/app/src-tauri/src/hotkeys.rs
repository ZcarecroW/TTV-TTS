//! Global hotkeys: they work while a game or OBS has the focus. Registered
//! from Rust (tauri-plugin-global-shortcut), configured in the settings.

use std::sync::Arc;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

/// What a hotkey can do.
pub const ACTIONS: [&str; 6] = ["skip", "pause", "clear", "replay", "approve", "reject"];

/// Checks an accelerator like "Ctrl+Alt+KeyS".
pub fn parse(accel: &str) -> Result<Shortcut, String> {
    accel.parse::<Shortcut>().map_err(|e| format!("\"{accel}\" isn't a key combination: {e}"))
}

/// Registers the configured hotkeys (replacing earlier ones). Returns the
/// ones the system refused (taken by another program).
pub fn apply(app: &AppHandle, state: &Arc<AppState>) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let keys = state.cfg.read().hotkeys.clone();
    let mut refused = Vec::new();
    for (action, accel) in keys {
        if accel.trim().is_empty() || !ACTIONS.contains(&action.as_str()) {
            continue;
        }
        let Ok(shortcut) = parse(&accel) else {
            refused.push(action);
            continue;
        };
        let st = state.clone();
        let a = action.clone();
        let r = gs.on_shortcut(shortcut, move |_app, _shortcut, ev| {
            if ev.state == ShortcutState::Pressed {
                run(&st, &a);
            }
        });
        if let Err(e) = r {
            log::warn!("hotkey {accel} for {action}: {e}");
            refused.push(action);
        }
    }
    refused
}

fn run(state: &Arc<AppState>, action: &str) {
    log::info!("hotkey: {action}");
    match action {
        "skip" => state.reader.skip(),
        "pause" => {
            let on = !state.reader.is_paused();
            state.reader.set_paused(on);
        }
        "clear" => state.reader.clear("hotkey"),
        "replay" => {
            if let Err(e) = state.reader.replay(None) {
                log::info!("replay: {e}");
            }
        }
        "approve" => {
            state.reader.approve(None);
        }
        "reject" => {
            state.reader.reject(None);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_the_settings_record() {
        assert!(parse("Ctrl+Alt+KeyS").is_ok());
        assert!(parse("Shift+F9").is_ok());
        assert!(parse("Ctrl+Shift+Digit1").is_ok());
        assert!(parse("Ctrl+").is_err());
        assert!(parse("Banana").is_err());
    }
}
