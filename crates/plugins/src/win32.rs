//! A plain Win32 window for a plugin editor to live in, and the message pump
//! of the plugin thread (the plugin's own child windows are served by it).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetWindowLongPtrW,
    IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW, SW_SHOW, SWP_NOMOVE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CLOSE, WM_NCDESTROY, WNDCLASSEXW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::{HSTRING, PCWSTR, w};

const CLASS: PCWSTR = w!("TtvTtsPluginEditor");
/// Not resizable by the user: the plugin decides its size.
const STYLE: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE = windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0);

/// A top-level window holding one plugin editor.
pub struct EditorWindow {
    pub hwnd: HWND,
    closed: Arc<AtomicBool>,
}

// The window lives and dies on the plugin thread; the handle is only data.
unsafe impl Send for EditorWindow {}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            // the host closes the editor first, then destroys the window
            let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AtomicBool;
            if !p.is_null() {
                unsafe { (*p).store(true, Ordering::SeqCst) };
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let p = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *const AtomicBool;
            if !p.is_null() {
                drop(unsafe { Arc::from_raw(p) });
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

impl EditorWindow {
    /// A window whose client area is `width` × `height` pixels.
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            // a second registration fails harmlessly
            RegisterClassExW(&wc);
            let mut r = RECT { left: 0, top: 0, right: width.max(50), bottom: height.max(30) };
            let _ = AdjustWindowRectEx(&mut r, STYLE, false, WINDOW_EX_STYLE(0));
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                CLASS,
                &HSTRING::from(title),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                r.right - r.left,
                r.bottom - r.top,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|e| format!("editor window: {e}"))?;
            let closed = Arc::new(AtomicBool::new(false));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Arc::into_raw(closed.clone()) as isize);
            Ok(Self { hwnd, closed })
        }
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    /// The user closed the window (the editor should be detached now).
    pub fn close_requested(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Resizes the client area.
    pub fn set_client_size(&self, width: i32, height: i32) {
        unsafe {
            let mut r = RECT { left: 0, top: 0, right: width.max(50), bottom: height.max(30) };
            let _ = AdjustWindowRectEx(&mut r, STYLE, false, WINDOW_EX_STYLE(0));
            let _ = SetWindowPos(self.hwnd, None, 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER);
        }
    }

    pub fn raw(&self) -> *mut std::ffi::c_void {
        self.hwnd.0
    }
}

impl Drop for EditorWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Serves all window messages waiting for this thread (call regularly while
/// an editor is open).
pub fn pump() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// How many child windows the plugin put into `hwnd` (a check that the
/// editor really attached).
pub fn child_count(hwnd: HWND) -> usize {
    use windows::Win32::Foundation::{LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;
    unsafe extern "system" fn count(_: HWND, l: LPARAM) -> windows::core::BOOL {
        unsafe { *(l.0 as *mut usize) += 1 };
        TRUE
    }
    let mut n = 0usize;
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(count), LPARAM(&mut n as *mut usize as isize));
    }
    n
}
