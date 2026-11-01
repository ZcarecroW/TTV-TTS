//! Window and tray icon. The proxy lives in the tray: closing the window only
//! hides it, "Quit" in the tray menu ends the proxy.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const TRAY: &str = "main";

pub fn open_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// UI language for the few texts Rust shows itself (tray menu).
pub fn lang(setting: &str) -> &'static str {
    match setting {
        "de" => "de",
        "en" => "en",
        _ => {
            #[cfg(windows)]
            {
                let mut buf = [0u16; 85];
                // SAFETY: buf is large enough for LOCALE_NAME_MAX_LENGTH.
                let n = unsafe { windows_sys::Win32::Globalization::GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
                let name = String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize]);
                if name.to_ascii_lowercase().starts_with("de") { "de" } else { "en" }
            }
            #[cfg(not(windows))]
            {
                let l = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")).unwrap_or_default();
                if l.to_ascii_lowercase().starts_with("de") { "de" } else { "en" }
            }
        }
    }
}

fn texts(lang: &str) -> [&'static str; 5] {
    if lang == "de" {
        ["TTV-TTS Proxy öffnen", "Gerät koppeln …", "VCP starten", "VCP neu starten", "Beenden"]
    } else {
        ["Open TTV-TTS Proxy", "Pair a device …", "Start VCP", "Restart VCP", "Quit"]
    }
}

pub fn setup_tray(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<()> {
    let t = texts(lang(&state.cfg.read().language));
    let open = MenuItem::with_id(app, "open", t[0], true, None::<&str>)?;
    let pair = MenuItem::with_id(app, "pair", t[1], true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", t[2], true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", t[3], true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t[4], true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &pair, &sep, &start, &restart, &sep2, &quit])?;
    let mut b = TrayIconBuilder::with_id(TRAY)
        .tooltip("TTV-TTS Proxy")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            let state = app.state::<Arc<AppState>>().inner().clone();
            match ev.id().as_ref() {
                "open" => open_window(app),
                "pair" => {
                    open_window(app);
                    let _ = app.emit("proxy://pair", ());
                }
                "start" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::vcp_watch::start(&state).await;
                    });
                }
                "restart" => {
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::vcp_watch::restart(&state).await;
                    });
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                open_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app)?;
    Ok(())
}

/// A device just paired: tell the window (it shows a confirmation).
pub fn paired(app: &AppHandle, device: &str) {
    let _ = app.emit("proxy://paired", device.to_string());
}
