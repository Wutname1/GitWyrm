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

// Cancel-confirmation overlay (centered card)
const CONFIRM_W: i32 = 360;
const CONFIRM_H: i32 = 160;
const CONFIRM_X: i32 = (WND_W - CONFIRM_W) / 2;
const CONFIRM_Y: i32 = (WND_H - CONFIRM_H) / 2;
const CONFIRM_BTN_W: i32 = 130;
const CONFIRM_BTN_H: i32 = 40;
const CONFIRM_BTN_GAP: i32 = 16;
const CONFIRM_YES_X: i32 = CONFIRM_X + CONFIRM_W - 24 - CONFIRM_BTN_W;
const CONFIRM_NO_X: i32 = CONFIRM_YES_X - CONFIRM_BTN_GAP - CONFIRM_BTN_W;
const CONFIRM_BTN_Y: i32 = CONFIRM_Y + CONFIRM_H - 24 - CONFIRM_BTN_H;

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
    /// Covering an in-place update rather than a first install.
    updating: bool,
    installing: bool,
    anim_tick: u64,
    hover_close_x: bool,
    hover_close_btn: bool,
    confirming_cancel: bool,
    hover_confirm_yes: bool,
    hover_confirm_no: bool,
    tracking_mouse: bool,
}

pub fn run(rx: std::sync::mpsc::Receiver<DownloadMsg>) {
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    // Cover an in-place update rather than a first install: no download, no
    // installer to run, just the wait while NSIS works and the app comes back.
    let updating = std::env::args().any(|a| a == "--updating");

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
        status: if updating {
            "Updating GitWyrm...".into()
        } else {
            "Downloading GitWyrm...".into()
        },
        detail: if updating {
            "This only takes a moment.".into()
        } else {
            String::new()
        },
        error: String::new(),
        font_title: create_font(-32, 700),
        font_tagline: create_font(-20, 600),
        font_body: create_font(-17, 600),
        font_small: create_font(-15, 400),
        font_small_bold: create_font(-16, 700),
        exiting: false,
        dry_run,
        updating,
        // Updating opens straight into the working state: there is no download
        // phase, so a zeroed progress bar would just look stalled.
        installing: updating,
        anim_tick: 0,
        hover_close_x: false,
        hover_close_btn: false,
        confirming_cancel: false,
        hover_confirm_yes: false,
        hover_confirm_no: false,
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

        // The taskbar label. "Setup" would misdescribe an update to someone who
        // already has the app, so this follows the same split as the heading.
        let title = HSTRING::from(if updating {
            "Updating GitWyrm"
        } else {
            "GitWyrm Setup"
        });

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name,
            PCWSTR(title.as_ptr()),
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

        // Started here rather than before the window exists, so the card is
        // already on screen when the watcher reports back -- otherwise a fast
        // update could finish before there was anything to cover the gap with.
        if updating {
            watch_for_relaunch((*state_ptr).tx.clone(), dry_run);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    std::process::exit(0);
}

/// Give up covering the update after this long.
///
/// The helper is a cover for a gap, not a supervisor: if NSIS wedges, the user
/// must get their screen back rather than staring at our card forever. Ten
/// minutes is far beyond the 20-40s the gap actually takes, so hitting this is
/// a real failure, not a slow disk.
const UPDATE_WATCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// How often to look for the relaunched app.
///
/// This is pure handover latency once the new window is up, so it is kept short.
/// Both probes are a process snapshot (plus, in phase 2, one `EnumWindows` pass)
/// -- cheap enough at 8/sec against a gap measured in tens of seconds.
const UPDATE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(125);

/// Cover the window between the old app exiting and the new one appearing.
///
/// Unlike the bootstrapper's own install, nothing here drives the installer:
/// the Tauri updater already handed it to ShellExecute before the app exited,
/// and NSIS `/UPDATE` relaunches GitWyrm itself. So this only watches, and the
/// signal it watches for is a *new* GitWyrm process -- not the exe file, which
/// exists the whole time and would end the cover the instant NSIS finished
/// writing, before there was a window to hand over to.
fn watch_for_relaunch(tx: Sender<DownloadMsg>, dry_run: bool) {
    std::thread::spawn(move || {
        if dry_run {
            std::thread::sleep(std::time::Duration::from_secs(4));
            let _ = tx.send(DownloadMsg::Installed);
            return;
        }

        let started = std::time::Instant::now();

        // Phase 1: wait for the OLD app to go.
        //
        // This window is spawned from the updater's on_before_exit hook, which
        // runs while the app that spawned it is still alive - so "is GitWyrm
        // running?" is true the moment this thread starts. Without waiting for
        // it to disappear first, the very first poll sees the *dying parent*,
        // decides the app is already back, and closes this window immediately.
        // That is exactly the flash the cover exists to prevent.
        while gitwyrm_is_running() {
            if started.elapsed() > UPDATE_WATCH_TIMEOUT {
                crate::log("ERROR: GitWyrm never exited; giving up");
                let _ = tx.send(DownloadMsg::Installed);
                return;
            }
            std::thread::sleep(UPDATE_POLL_INTERVAL);
        }
        crate::log("Old GitWyrm has exited; waiting for the update to finish");

        // Phase 2: wait for the NEW app to appear.
        loop {
            if started.elapsed() > UPDATE_WATCH_TIMEOUT {
                crate::log("ERROR: timed out waiting for GitWyrm to reappear");
                let _ = tx.send(DownloadMsg::Error(
                    "The update is taking longer than expected.\n\nIt may still finish on its own. \
                     If GitWyrm does not reopen, launch it from the Start Menu."
                        .into(),
                ));
                return;
            }

            // A process is not a window: the relaunched app spends its first
            // seconds behind its own boot splash restoring tabs. Handing over on
            // the process alone leaves this card sitting on screen with nothing
            // to swap to, which reads as the cover being slow to close.
            if gitwyrm_window_is_up() {
                crate::log("GitWyrm has a window again; handing over");
                let _ = tx.send(DownloadMsg::Installed);
                return;
            }

            std::thread::sleep(UPDATE_POLL_INTERVAL);
        }
    });
}

/// Whether any GitWyrm process is alive.
///
/// Phase 1 of the watch only needs to know the *old* app has gone, and a dying
/// process loses its window well before it loses its process entry -- so this
/// stays a process check, deliberately.
fn gitwyrm_is_running() -> bool {
    !gitwyrm_pids().is_empty()
}

/// Process IDs of every running GitWyrm.
fn gitwyrm_pids() -> Vec<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::*;

    let mut pids = Vec::new();

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if name.eq_ignore_ascii_case(crate::APP_EXE_NAME) {
                    pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    pids
}

/// Collects a visible top-level window belonging to one of the target PIDs.
struct WindowProbe {
    pids: Vec<u32>,
    found: bool,
}

/// Whether the relaunched app has a visible window yet.
///
/// This is the handover signal rather than "the process exists" because the app
/// spends its first seconds starting up with nothing on screen. Swapping on the
/// process would leave this card up over an empty desktop -- the very gap it is
/// here to cover -- and make the close look sluggish.
fn gitwyrm_window_is_up() -> bool {
    let pids = gitwyrm_pids();
    if pids.is_empty() {
        return false;
    }

    let mut probe = WindowProbe { pids, found: false };

    unsafe {
        let _ = EnumWindows(
            Some(probe_window),
            LPARAM(&mut probe as *mut WindowProbe as isize),
        );
    }

    probe.found
}

unsafe extern "system" fn probe_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let probe = &mut *(lparam.0 as *mut WindowProbe);

    if IsWindowVisible(hwnd).as_bool() {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if probe.pids.contains(&pid) {
            probe.found = true;
            // Returning FALSE stops the enumeration; we have our answer.
            return BOOL(0);
        }
    }

    BOOL(1)
}

/// Fetch the extra tools the app needs (git, gpg, whatever the manifest lists)
/// after the app itself is installed.
///
/// Failure here never fails the install. The app re-checks the same manifest on
/// launch and falls back to whatever git is on PATH, so the worst case is a
/// slower first run - not a broken one. That is why every path logs and returns
/// rather than sending `Error`, which would put the window into its failure
/// state over something already recoverable.
fn install_components(tx: &Sender<DownloadMsg>) {
    let client = match crate::http_client() {
        Ok(c) => c,
        Err(e) => {
            crate::log(&format!("WARNING: no HTTP client for components: {e}"));
            return;
        }
    };

    let pending = crate::components::pending(&client);
    if pending.is_empty() {
        crate::log("No components to install");
        return;
    }

    for component in &pending {
        if let Err(e) = crate::components::install(&client, component, tx) {
            crate::log(&format!("WARNING: {} did not install: {e}", component.name));
        }
    }
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
                    install_components(&tx);
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
                        s.status = if s.updating {
                            "Update complete".into()
                        } else {
                            "Launching GitWyrm...".into()
                        };
                        s.detail.clear();
                        s.installing = false;
                        let _ = InvalidateRect(Some(hwnd), None, false);

                        // In updating mode the app is already back: NSIS `/UPDATE`
                        // relaunched it, and that is the very thing the watcher
                        // waited to see. Launching again would hand a second
                        // process to the single-instance plugin for nothing.
                        let launch_failed = if !s.dry_run && !s.updating {
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
                            // No sleep before destroying: this runs on the message
                            // thread, so waiting here freezes the card mid-close
                            // instead of letting it disappear. Anything that needs
                            // to settle first belongs in the watcher thread.
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

            // Title bar (icon + wordmark, over the right panel)
            draw_logo(mem_dc, PANEL_W + 24, 12, 32, 32);
            let wordmark_w = draw_wordmark_img(mem_dc, PANEL_W + 68, 16, 24);
            // "Setup" belongs to a first install; during an update the wordmark
            // alone is right, and the taskbar title matches it (see below).
            if !s.updating {
                draw_text(mem_dc, " Setup", PANEL_W + 68 + wordmark_w, 18, 100, 24, s.font_body, COLOR_TEXT);
            }
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
                // An update is not a first meeting: someone mid-update already
                // has GitWyrm and does not need to be sold it, so the welcome
                // and the pitch give way to what is happening and why the app
                // just vanished off their screen.
                let heading = if s.updating { "Updating" } else { "Welcome to" };
                draw_text(mem_dc, heading, content_x, 110, content_w, 44, s.font_title, COLOR_TEXT);
                let wordmark_w = draw_wordmark_img(mem_dc, content_x, 156, 40);
                if !s.updating {
                    draw_text(mem_dc, " Setup", content_x + wordmark_w, 154, content_w - wordmark_w, 44, s.font_title, COLOR_TEXT);
                }

                let tagline = if s.updating {
                    "Hang tight."
                } else {
                    "Fast. Focused. Familiar."
                };
                draw_text(mem_dc, tagline, content_x, 212, content_w, 30, s.font_tagline, COLOR_ACCENT);

                let blurb = if s.updating {
                    "GitWyrm is installing the new version and will reopen on its own. Your tabs and repositories are exactly as you left them."
                } else {
                    "GitWyrm brings a fast, familiar, and beautiful experience to your Git workflows."
                };
                draw_text_wrap(
                    mem_dc,
                    blurb,
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

            if s.confirming_cancel {
                fill_rect(mem_dc, 0, 0, WND_W, WND_H, COLOR_SCRIM);
                fill_rounded_rect(mem_dc, CONFIRM_X, CONFIRM_Y, CONFIRM_W, CONFIRM_H, 10, COLOR_PANEL);
                draw_text(mem_dc, "Cancel setup?", CONFIRM_X + 24, CONFIRM_Y + 24, CONFIRM_W - 48, 28, s.font_body, COLOR_TEXT);
                draw_text_wrap(
                    mem_dc,
                    "GitWyrm has not finished installing yet.",
                    CONFIRM_X + 24,
                    CONFIRM_Y + 56,
                    CONFIRM_W - 48,
                    40,
                    s.font_small,
                    COLOR_SUBTEXT,
                );

                let no_bg = if s.hover_confirm_no { COLOR_HOVER } else { COLOR_BAR_BG };
                fill_rounded_rect(mem_dc, CONFIRM_NO_X, CONFIRM_BTN_Y, CONFIRM_BTN_W, CONFIRM_BTN_H, 8, no_bg);
                draw_text_center(mem_dc, "Keep going", CONFIRM_NO_X, CONFIRM_BTN_Y, CONFIRM_BTN_W, CONFIRM_BTN_H, s.font_small_bold, COLOR_TEXT);

                let yes_bg = if s.hover_confirm_yes { COLOR_ERROR } else { COLOR_BAR_BG };
                fill_rounded_rect(mem_dc, CONFIRM_YES_X, CONFIRM_BTN_Y, CONFIRM_BTN_W, CONFIRM_BTN_H, 8, yes_bg);
                draw_text_center(mem_dc, "Cancel setup", CONFIRM_YES_X, CONFIRM_BTN_Y, CONFIRM_BTN_W, CONFIRM_BTN_H, s.font_small_bold, COLOR_TEXT);
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

            if s.confirming_cancel {
                if click_x >= CONFIRM_YES_X
                    && click_x < CONFIRM_YES_X + CONFIRM_BTN_W
                    && click_y >= CONFIRM_BTN_Y
                    && click_y < CONFIRM_BTN_Y + CONFIRM_BTN_H
                {
                    s.exiting = true;
                    KillTimer(Some(hwnd), 1).ok();
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }

                if click_x >= CONFIRM_NO_X
                    && click_x < CONFIRM_NO_X + CONFIRM_BTN_W
                    && click_y >= CONFIRM_BTN_Y
                    && click_y < CONFIRM_BTN_Y + CONFIRM_BTN_H
                {
                    s.confirming_cancel = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }

                // Swallow all other clicks while the overlay is up (modal).
                return LRESULT(0);
            }

            if click_x >= CLOSE_X && click_x < CLOSE_X + CLOSE_W && click_y >= CLOSE_Y && click_y < CLOSE_Y + CLOSE_H {
                if s.error.is_empty() {
                    s.confirming_cancel = true;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else {
                    s.exiting = true;
                    KillTimer(Some(hwnd), 1).ok();
                    let _ = DestroyWindow(hwnd);
                }
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

            if s.confirming_cancel {
                let over_yes = mx >= CONFIRM_YES_X
                    && mx < CONFIRM_YES_X + CONFIRM_BTN_W
                    && my >= CONFIRM_BTN_Y
                    && my < CONFIRM_BTN_Y + CONFIRM_BTN_H;
                let over_no = mx >= CONFIRM_NO_X
                    && mx < CONFIRM_NO_X + CONFIRM_BTN_W
                    && my >= CONFIRM_BTN_Y
                    && my < CONFIRM_BTN_Y + CONFIRM_BTN_H;

                if over_yes != s.hover_confirm_yes || over_no != s.hover_confirm_no {
                    s.hover_confirm_yes = over_yes;
                    s.hover_confirm_no = over_no;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }

                return LRESULT(0);
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
            if s.hover_close_x || s.hover_close_btn || s.hover_confirm_yes || s.hover_confirm_no {
                s.hover_close_x = false;
                s.hover_close_btn = false;
                s.hover_confirm_yes = false;
                s.hover_confirm_no = false;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }

        // Drag the borderless window by the title bar area (right panel only, avoid the splash image
        // and the close button - the close button must stay a real client-area hit so it gets
        // WM_LBUTTONUP instead of being swallowed as a caption drag).
        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let local_x = x - rect.left;
            let local_y = y - rect.top;
            let over_close = local_x >= CLOSE_X && local_x < CLOSE_X + CLOSE_W && local_y >= CLOSE_Y && local_y < CLOSE_Y + CLOSE_H;
            if !over_close && local_y >= 0 && local_y < TITLEBAR_H && local_x >= PANEL_W && local_x < WND_W {
                LRESULT(2) // HTCAPTION
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window probe must never claim a window when no matching process
    /// exists -- that would end the cover over an empty desktop.
    #[test]
    fn window_probe_requires_a_matching_process() {
        if gitwyrm_pids().is_empty() {
            assert!(
                !gitwyrm_window_is_up(),
                "no GitWyrm process, so no GitWyrm window can be reported"
            );
        }
    }

    /// Phase 2 is strictly stronger than phase 1: a visible window implies a
    /// live process. If this ever inverted, the cover would hand over to
    /// something that had already gone.
    #[test]
    fn a_window_implies_a_running_process() {
        if gitwyrm_window_is_up() {
            assert!(
                gitwyrm_is_running(),
                "a reported window must belong to a running process"
            );
        }
    }

    /// Handover latency is the poll interval, so it must stay well under the
    /// point a user reads the close as sluggish.
    #[test]
    fn handover_latency_is_not_perceptible() {
        assert!(
            UPDATE_POLL_INTERVAL <= std::time::Duration::from_millis(200),
            "poll interval is pure handover delay once the new window is up"
        );
    }
}
