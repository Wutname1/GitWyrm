//! Windows Snap Layouts support for the custom title bar.
//!
//! Hovering the maximize button for a moment is supposed to open the Snap
//! Layouts flyout. That is a shell feature, not a window one: the shell asks the
//! window what is under the cursor with `WM_NCHITTEST` and only offers the
//! flyout when the reply is `HTMAXBUTTON`. GitWyrm draws its own caption
//! (`decorations: false`), so the maximize button is an HTML element and the
//! window never gives that answer -- the shell never learns a maximize button
//! exists, and the flyout never appears.
//!
//! Subclassing the *top-level* window and answering `HTMAXBUTTON` there does not
//! work, though it is the obvious first attempt. WebView2 puts its own child
//! window over the whole client area, and that child takes the mouse messages,
//! so the top-level window is never asked to hit-test the pixels the cursor is
//! actually over. Logging every `WM_NCHITTEST` on the parent while hovering the
//! caption buttons produces nothing at all -- not a wrong answer, no question.
//!
//! So the answer has to come from a window that really is under the cursor: a
//! small child window, owned by us, positioned exactly over the button and above
//! the webview. It is never painted -- it exists only to answer `HTMAXBUTTON` --
//! so the React button still draws itself and the overlay is invisible.
//!
//! Being on top means it also swallows the hover and the click that used to
//! reach the DOM. Both are handed back as events -- `snap-layouts://hover` and
//! `snap-layouts://click` -- which `WindowControls` uses to drive the same
//! states it would otherwise get from the browser.
//!
//! Everything here is a no-op off Windows.

use serde::Deserialize;
use tauri::WebviewWindow;

/// The maximize button's bounds, in CSS pixels relative to the client area.
#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
pub struct ButtonRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Tell Snap Layouts where the maximize button is, or that there is not one.
///
/// Called whenever the button moves or resizes -- the title bar reflows as tabs
/// are opened, and the window itself is resizable.
///
/// The overlay is created here, on the first report, rather than at startup.
/// Every window that wants Snap Layouts is a window that draws a maximize button
/// and therefore calls this, which covers the Spec Desk windows opened later
/// without a second hook, and leaves any window without a button (the crash bar)
/// with nothing overlaid on it. Passing `None` hides the overlay, so a title bar
/// that unmounts stops claiming its pixels.
#[tauri::command]
#[specta::specta]
pub fn set_maximize_button_rect(window: WebviewWindow, rect: Option<ButtonRect>) {
    imp::set_button_rect(&window, rect);
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use tauri::WebviewWindow;

    pub fn set_button_rect(_window: &WebviewWindow, _rect: Option<super::ButtonRect>) {}
}

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use tauri::{Emitter, WebviewWindow};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, NULL_BRUSH};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, RegisterClassExW, SetWindowLongPtrW,
        SetWindowPos, ShowWindow, GWLP_USERDATA, HTMAXBUTTON, HWND_TOP, SWP_NOACTIVATE, SW_HIDE,
        SW_SHOWNOACTIVATE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP,
        WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
    };

    /// One overlay per top-level window, keyed by the parent's handle.
    static OVERLAYS: Mutex<Option<HashMap<isize, isize>>> = Mutex::new(None);

    /// Whether the window class has been registered. Registering twice fails, and
    /// the class outlives every window built from it, so it is done once.
    static CLASS: Mutex<bool> = Mutex::new(false);

    /// The overlay's window class name, NUL-terminated UTF-16 for the W APIs.
    fn class_name() -> Vec<u16> {
        "GitWyrmSnapLayout\0".encode_utf16().collect()
    }

    /// State the overlay's procedure needs, hung off the window itself via
    /// `GWLP_USERDATA` so no lookup (and no lock) is needed to service a message.
    struct State {
        window: WebviewWindow,
        /// Whether the cursor is inside, so hover events fire on the edges rather
        /// than on every mouse move.
        hovered: bool,
        /// Whether the press started here, so a drag that ends over the button
        /// does not count as a click.
        pressing: bool,
    }

    fn with_overlays<T>(f: impl FnOnce(&mut HashMap<isize, isize>) -> T) -> T {
        let mut guard = OVERLAYS.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(HashMap::new))
    }

    /// Register the overlay's window class, once per process.
    unsafe fn ensure_class(name: &[u16]) -> bool {
        let mut registered = CLASS.lock().unwrap_or_else(|e| e.into_inner());
        if *registered {
            return true;
        }
        let mut class: WNDCLASSEXW = std::mem::zeroed();
        class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        class.lpfnWndProc = Some(overlay_proc);
        class.hInstance = GetModuleHandleW(std::ptr::null());
        class.lpszClassName = name.as_ptr();
        // A null brush is what keeps the overlay invisible: the window never
        // paints, so whatever the webview drew stays on screen. (A layered
        // window would be the textbook way, but WS_EX_LAYERED on this child is
        // refused outright -- CreateWindowExW returns null with no error code.)
        class.hbrBackground = GetStockObject(NULL_BRUSH) as HBRUSH;
        let atom = RegisterClassExW(&class);
        if atom == 0 {
            log::warn!(
                "snap layouts: could not register the overlay class, GetLastError={}",
                windows_sys::Win32::Foundation::GetLastError()
            );
            return false;
        }
        *registered = true;
        true
    }

    /// Create the invisible overlay over `parent`.
    unsafe fn create_overlay(parent: HWND, window: &WebviewWindow) -> Option<HWND> {
        let name = class_name();
        if !ensure_class(&name) {
            return None;
        }
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            log::warn!(
                "snap layouts: could not create the overlay window, GetLastError={}",
                windows_sys::Win32::Foundation::GetLastError()
            );
            return None;
        }

        let state = Box::new(State {
            window: window.clone(),
            hovered: false,
            pressing: false,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        log::info!(
            "snap layouts: overlay ready for window {:#x}",
            parent as isize
        );
        Some(hwnd)
    }

    pub fn set_button_rect(window: &WebviewWindow, rect: Option<super::ButtonRect>) {
        // Tauri commands run on a worker thread, but a Win32 window belongs to
        // the thread that creates it -- created here it would have no message
        // pump and never hear a single message. Hop to the UI thread, where the
        // parent lives and the message loop runs.
        //
        // Everything the closure needs is fetched HERE, before the hop. Tauri's
        // window getters (`hwnd`, `scale_factor`) post a request to the event
        // loop and block for the answer -- called from inside a closure the
        // event loop is currently running, that wait never ends and the whole
        // app freezes at startup.
        let Ok(parent) = window.hwnd() else { return };
        let parent = parent.0 as isize;
        let scale = window.scale_factor().unwrap_or(1.0);
        let win = window.clone();
        let result = window.run_on_main_thread(move || {
            set_button_rect_on_ui_thread(&win, parent, rect, scale);
        });
        if let Err(e) = result {
            log::warn!("snap layouts: could not reach the UI thread: {e}");
        }
    }

    fn set_button_rect_on_ui_thread(
        window: &WebviewWindow,
        parent: isize,
        rect: Option<super::ButtonRect>,
        scale: f64,
    ) {
        let parent = parent as HWND;
        let key = parent as isize;

        unsafe {
            let hwnd = match with_overlays(|m| m.get(&key).copied()) {
                Some(h) => h as HWND,
                None => {
                    // Nothing to place and no overlay yet: leave it that way
                    // rather than creating a window only to hide it.
                    if rect.is_none() {
                        return;
                    }
                    let Some(h) = create_overlay(parent, window) else {
                        return;
                    };
                    with_overlays(|m| m.insert(key, h as isize));
                    h
                }
            };

            match rect {
                Some(r) => {
                    SetWindowPos(
                        hwnd,
                        HWND_TOP,
                        (r.x * scale).round() as i32,
                        (r.y * scale).round() as i32,
                        (r.width * scale).round() as i32,
                        (r.height * scale).round() as i32,
                        SWP_NOACTIVATE,
                    );
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                None => {
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }

    unsafe extern "system" fn overlay_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if state_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &mut *state_ptr;

        // Answering HTMAXBUTTON makes every later mouse message arrive in its
        // NON-client form (WM_NCMOUSEMOVE, not WM_MOUSEMOVE) -- the hit test
        // decides which family the OS sends. Handling the client family here
        // would compile fine and never fire.
        match msg {
            // The whole point: this is what makes the shell offer Snap Layouts.
            WM_NCHITTEST => HTMAXBUTTON as LRESULT,
            WM_NCMOUSEMOVE => {
                if !state.hovered {
                    state.hovered = true;
                    let _ = state.window.emit("snap-layouts://hover", true);
                }
                // Re-armed on every move: TrackMouseEvent is one-shot, and
                // without it there is no leave message, so the button would
                // stay lit after the cursor moved away. TME_NONCLIENT to match
                // the message family the hit test put us in.
                let mut track: TRACKMOUSEEVENT = std::mem::zeroed();
                track.cbSize = std::mem::size_of::<TRACKMOUSEEVENT>() as u32;
                track.dwFlags = TME_LEAVE | TME_NONCLIENT;
                track.hwndTrack = hwnd;
                TrackMouseEvent(&mut track);
                0
            }
            WM_NCMOUSELEAVE => {
                state.pressing = false;
                if state.hovered {
                    state.hovered = false;
                    let _ = state.window.emit("snap-layouts://hover", false);
                }
                0
            }
            // Swallowed rather than chained: the default handler would treat a
            // press on a caption button as the start of something else.
            WM_NCLBUTTONDOWN => {
                state.pressing = true;
                0
            }
            WM_NCLBUTTONUP => {
                if state.pressing {
                    state.pressing = false;
                    let _ = state.window.emit("snap-layouts://click", ());
                }
                0
            }
            WM_NCDESTROY => {
                // Reclaim the state and clear the pointer before the window goes,
                // so a late message cannot reach freed memory.
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state_ptr));
                with_overlays(|m| m.retain(|_, v| *v != hwnd as isize));
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
