use std::ffi::c_void;
use std::sync::mpsc::Sender;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::paint::*;
use crate::DownloadMsg;

const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
const WND_W: i32 = 1080;
const WND_H: i32 = 720;
const PANEL_W: i32 = 500; // left splash panel width
const TITLEBAR_H: i32 = 56;

// Close button rect (top-right corner)
const CLOSE_X: i32 = WND_W - 44;
const CLOSE_Y: i32 = 12;
const CLOSE_W: i32 = 32;
const CLOSE_H: i32 = 32;

// "Close" text button on error screen (bottom of right panel)
const ERR_BTN_W: i32 = 110;
const ERR_BTN_H: i32 = 44;

struct AppState {
    tx: Sender<DownloadMsg>,
    rx: std::sync::mpsc::Receiver<DownloadMsg>,
    progress: f64,
    status: String,
    detail: String,
    error: String,
    font_title: HFONT,
    font_tagline: HFONT,
    font_body: HFONT,
    font_small: HFONT,
    font_small_bold: HFONT,
    exiting: bool,
    dry_run: bool,
    installing: bool,
    anim_tick: u64,
    hover_close_x: bool,
    hover_close_btn: bool,
    tracking_mouse: bool,
}

pub fn run(rx: std::sync::mpsc::Receiver<DownloadMsg>) {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    // Replace the original channel with one we control,
    // so both download and install threads can send to it
    let (tx, unified_rx) = std::sync::mpsc::channel::<DownloadMsg>();

    let relay_tx = tx.clone();
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            let _ = relay_tx.send(msg);
        }
    });

    let app = Box::new(AppState {
        tx,
        rx: unified_rx,
        progress: 0.0,
        status: "Downloading GitWyrm...".into(),
        detail: String::new(),
        error: String::new(),
        font_title: create_font(-32, 700),
        font_tagline: create_font(-20, 600),
        font_body: create_font(-17, 600),
        font_small: create_font(-15, 400),
        font_small_bold: create_font(-16, 700),
        exiting: false,
        dry_run,
        installing: false,
        anim_tick: 0,
        hover_close_x: false,
        hover_close_btn: false,
        tracking_mouse: false,
    });

    let state_ptr = Box::into_raw(app);

    unsafe {
        let class_name = w!("GitWyrmSetup");
        let hinstance = GetModuleHandleW(None).unwrap();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };

        RegisterClassExW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            w!("GitWyrm Setup"),
            WS_POPUP | WS_VISIBLE,
            (screen_w - WND_W) / 2,
            (screen_h - WND_H) / 2,
            WND_W,
            WND_H,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .unwrap();

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

        // Dark title bar + rounded corners (Win11)
        let dark: BOOL = true.into();
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const c_void,
            std::mem::size_of::<BOOL>() as u32,
        );
        let corner = DWM_WINDOW_CORNER_PREFERENCE(3);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );

        SetTimer(Some(hwnd), 1, 50, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    std::process::exit(0);
}

fn run_silent_install(path: std::path::PathBuf, tx: Sender<DownloadMsg>, dry_run: bool) {
    std::thread::spawn(move || {
        if dry_run {
            let _ = tx.send(DownloadMsg::Progress(0, 1));
            std::thread::sleep(std::time::Duration::from_secs(3));
            let _ = tx.send(DownloadMsg::Installed);
            return;
        }

        if !path.exists() {
            crate::log(&format!("ERROR: Installer not found at {}", path.display()));
            let _ = tx.send(DownloadMsg::Error("Installer file not found".into()));
            return;
        }

        crate::log(&format!("Running installer: {} /S", path.display()));
        match std::process::Command::new(&path).arg("/S").status() {
            Ok(status) => {
                crate::log(&format!("Installer exited with code: {:?}", status.code()));
                let _ = std::fs::remove_file(&path);
                if status.success() {
                    let _ = tx.send(DownloadMsg::Installed);
                } else {
                    let msg = format!("Installer exited with code {}", status.code().unwrap_or(-1));
                    crate::log(&format!("ERROR: {}", msg));
                    let _ = tx.send(DownloadMsg::Error(msg));
                }
            }
            Err(e) => {
                let msg = format!("Failed to run installer: {}", e);
                crate::log(&format!("ERROR: {}", msg));
                let _ = tx.send(DownloadMsg::Error(msg));
            }
        }
    });
}

fn launch_app() -> Option<String> {
    let local_app_data = match std::env::var_os("LOCALAPPDATA") {
        Some(v) => v,
        None => return Some("LOCALAPPDATA not set".into()),
    };
    let base = std::path::Path::new(&local_app_data);

    let candidates = [base.join("GitWyrm").join(crate::APP_EXE_NAME)];

    for path in &candidates {
        crate::log(&format!("Checking: {} exists={}", path.display(), path.exists()));
        if path.exists() {
            match std::process::Command::new(path).spawn() {
                Ok(_) => {
                    crate::log(&format!("Launched: {}", path.display()));
                    return None;
                }
                Err(e) => {
                    let msg = format!("Failed to launch {}: {}", path.display(), e);
                    crate::log(&format!("ERROR: {}", msg));
                    return Some(msg);
                }
            }
        }
    }

    let msg = format!(
        "App not found. Checked:\n{}",
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")
    );
    crate::log(&format!("ERROR: {}", msg));
    Some(msg)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

    if ptr.is_null() {
        if msg == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            fill_rect(hdc, 0, 0, WND_W, WND_H, COLOR_BG);
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let s = &mut *ptr;
    if s.exiting {
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    match msg {
        WM_TIMER => {
            while let Ok(m) = s.rx.try_recv() {
                match m {
                    DownloadMsg::Progress(dl, total) => {
                        if total > 0 {
                            s.progress = dl as f64 / total as f64;
                            s.detail = format!(
                                "{:.1} MB / {:.1} MB  ({:.0}%)",
                                dl as f64 / 1_048_576.0,
                                total as f64 / 1_048_576.0,
                                s.progress * 100.0,
                            );
                        } else {
                            s.detail = format!("{:.1} MB downloaded", dl as f64 / 1_048_576.0);
                        }
                    }
                    DownloadMsg::Done(path) => {
                        s.status = "Installing...".into();
                        s.detail.clear();
                        s.progress = 0.0;
                        s.installing = true;
                        s.anim_tick = 0;
                        let _ = InvalidateRect(Some(hwnd), None, false);

                        run_silent_install(path, s.tx.clone(), s.dry_run);
                    }
                    DownloadMsg::Installed => {
                        s.status = "Launching GitWyrm...".into();
                        s.detail.clear();
                        s.installing = false;
                        let _ = InvalidateRect(Some(hwnd), None, false);

                        let launch_failed = if !s.dry_run {
                            match launch_app() {
                                None => false,
                                Some(e) => {
                                    s.status.clear();
                                    s.error = format!(
                                        "Install succeeded but could not launch:\n{}\n\nYou can launch GitWyrm from the Start Menu.",
                                        e
                                    );
                                    let _ = InvalidateRect(Some(hwnd), None, false);
                                    true
                                }
                            }
                        } else {
                            false
                        };

                        if !launch_failed {
                            s.exiting = true;
                            KillTimer(Some(hwnd), 1).ok();
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            let _ = DestroyWindow(hwnd);
                            return LRESULT(0);
                        }
                    }
                    DownloadMsg::Error(err) => {
                        s.status.clear();
                        s.detail.clear();
                        s.progress = 0.0;
                        s.installing = false;
                        s.error = format!(
                            "Could not install GitWyrm:\n{}\n\nPlease try again or download from gitwyrm.com",
                            err
                        );
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            if s.installing {
                s.anim_tick += 1;
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Double-buffer
            let mem_dc = CreateCompatibleDC(Some(hdc));
            let bmp = CreateCompatibleBitmap(hdc, WND_W, WND_H);
            let old = SelectObject(mem_dc, bmp.into());

            fill_rect(mem_dc, 0, 0, WND_W, WND_H, COLOR_BG);

            // Left splash panel (fills full height behind the title bar)
            draw_splash(mem_dc, 0, 0, PANEL_W, WND_H);

            // Right content panel background
            fill_rect(mem_dc, PANEL_W, 0, WND_W - PANEL_W, WND_H, COLOR_PANEL);

            // Title bar (icon + product name, over the right panel)
            draw_logo(mem_dc, PANEL_W + 24, 12, 32, 32);
            draw_text(mem_dc, "GitWyrm Setup", PANEL_W + 68, 18, 300, 24, s.font_body, COLOR_TEXT);
            fill_rect(mem_dc, PANEL_W, TITLEBAR_H, WND_W - PANEL_W, 1, COLOR_DIVIDER);

            // X close button (top-right)
            let x_color = if s.hover_close_x { COLOR_HOVER } else { COLOR_SUBTEXT };
            draw_text_center(mem_dc, "\u{00D7}", CLOSE_X, CLOSE_Y, CLOSE_W, CLOSE_H, s.font_body, x_color);

            let content_x = PANEL_W + 56;
            let content_w = WND_W - PANEL_W - 112;

            if !s.error.is_empty() {
                draw_text(mem_dc, "Setup failed", content_x, 120, content_w, 44, s.font_title, COLOR_TEXT);
                draw_text_wrap(mem_dc, &s.error, content_x, 190, content_w, 240, s.font_small_bold, COLOR_ERROR);

                let btn_y = WND_H - 56 - ERR_BTN_H;
                let btn_bg = if s.hover_close_btn { COLOR_HOVER } else { COLOR_BAR_BG };
                fill_rounded_rect(mem_dc, content_x, btn_y, ERR_BTN_W, ERR_BTN_H, 8, btn_bg);
                draw_text_center(mem_dc, "Close", content_x, btn_y, ERR_BTN_W, ERR_BTN_H, s.font_small_bold, COLOR_TEXT);
            } else {
                draw_text(mem_dc, "Welcome to", content_x, 110, content_w, 44, s.font_title, COLOR_TEXT);
                draw_text(mem_dc, "GitWyrm Setup", content_x, 154, content_w, 44, s.font_title, COLOR_TEXT);

                draw_text(mem_dc, "Fast. Focused. Familiar.", content_x, 212, content_w, 30, s.font_tagline, COLOR_ACCENT);

                draw_text_wrap(
                    mem_dc,
                    "GitWyrm brings a fast, familiar, and beautiful experience to your Git workflows.",
                    content_x,
                    250,
                    content_w,
                    50,
                    s.font_small,
                    COLOR_SUBTEXT,
                );

                fill_rect(mem_dc, content_x, 320, content_w, 1, COLOR_DIVIDER);

                // Status + progress bar anchored near the bottom of the right panel
                let bar_y = WND_H - 56 - 20 - 16;

                if !s.status.is_empty() {
                    draw_text(mem_dc, &s.status, content_x, bar_y - 30, content_w, 26, s.font_body, COLOR_TEXT);
                }
                fill_rounded_rect(mem_dc, content_x, bar_y, content_w, 16, 8, COLOR_BAR_BG);
                if s.installing {
                    fill_indeterminate_bar(mem_dc, content_x, bar_y, content_w, 16, 8, COLOR_BAR_START, COLOR_BAR_END, s.anim_tick);
                } else if s.progress > 0.001 {
                    let fill_w = ((content_w as f64) * s.progress) as i32;
                    if fill_w > 0 {
                        fill_gradient_bar(mem_dc, content_x, bar_y, fill_w, 16, 8, COLOR_BAR_START, COLOR_BAR_END);
                    }
                }

                if !s.detail.is_empty() {
                    draw_text(mem_dc, &s.detail, content_x, bar_y + 24, content_w, 20, s.font_small, COLOR_SUBTEXT);
                }
            }

            let _ = BitBlt(hdc, 0, 0, WND_W, WND_H, Some(mem_dc), 0, 0, SRCCOPY);

            SelectObject(mem_dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_ERASEBKGND => LRESULT(1),

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            let click_x = (lparam.0 & 0xFFFF) as i16 as i32;
            let click_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            if click_x >= CLOSE_X && click_x < CLOSE_X + CLOSE_W && click_y >= CLOSE_Y && click_y < CLOSE_Y + CLOSE_H {
                s.exiting = true;
                KillTimer(Some(hwnd), 1).ok();
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }

            if !s.error.is_empty() {
                let content_x = PANEL_W + 56;
                let btn_y = WND_H - 56 - ERR_BTN_H;
                if click_x >= content_x
                    && click_x < content_x + ERR_BTN_W
                    && click_y >= btn_y
                    && click_y < btn_y + ERR_BTN_H
                {
                    s.exiting = true;
                    KillTimer(Some(hwnd), 1).ok();
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
            }

            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let mx = (lparam.0 & 0xFFFF) as i16 as i32;
            let my = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            if !s.tracking_mouse {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                s.tracking_mouse = true;
            }

            let over_x = mx >= CLOSE_X && mx < CLOSE_X + CLOSE_W && my >= CLOSE_Y && my < CLOSE_Y + CLOSE_H;
            let over_btn = if !s.error.is_empty() {
                let content_x = PANEL_W + 56;
                let btn_y = WND_H - 56 - ERR_BTN_H;
                mx >= content_x && mx < content_x + ERR_BTN_W && my >= btn_y && my < btn_y + ERR_BTN_H
            } else {
                false
            };

            if over_x != s.hover_close_x || over_btn != s.hover_close_btn {
                s.hover_close_x = over_x;
                s.hover_close_btn = over_btn;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            LRESULT(0)
        }

        WM_MOUSELEAVE_MSG => {
            s.tracking_mouse = false;
            if s.hover_close_x || s.hover_close_btn {
                s.hover_close_x = false;
                s.hover_close_btn = false;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }

        // Drag the borderless window by the title bar area (right panel only, avoid the splash image)
        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let local_x = x - rect.left;
            let local_y = y - rect.top;
            if local_y >= 0 && local_y < TITLEBAR_H && local_x >= PANEL_W && local_x < WND_W {
                LRESULT(2) // HTCAPTION
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
