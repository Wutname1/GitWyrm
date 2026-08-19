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

// Install mode: a wide two-panel card -- splash art on the left, the welcome
// copy and progress on the right.
const INSTALL_W: i32 = 1080;
const INSTALL_H: i32 = 720;
const PANEL_W: i32 = 500; // left splash panel width

// Update mode: the splash art and nothing else.
//
// An update has nothing to say. There is no welcome, no pitch and no choice to
// make -- the app vanished for a moment and is coming back. Sizing the window
// to the art plus a status strip removes the empty right-hand panel that made
// the old card look like a form waiting to be filled in.
//
// The art is 586x841. Drawn at its own aspect ratio the card comes out taller
// than a phone, which is far more screen than "back in a moment" deserves, so
// the art panel is deliberately shorter than the source and cover-fit crops it
// to a landscape band -- the dragon sits in the middle of the composition, so
// what a centred crop keeps is the part worth showing.
const UPDATE_W: i32 = 350;
/// Icon + wordmark band, drawn over the top of the art.
const UPDATE_HEADER_H: i32 = 50;
/// Bottom of the art panel. The header sits inside this, so the art actually
/// visible is `UPDATE_ART_H - UPDATE_HEADER_H` tall.
const UPDATE_ART_H: i32 = 500 + UPDATE_HEADER_H;
// Status line + progress bar below the art. Sized so the padding above the
// caption matches the clearance under the bar -- the strip is only two rows of
// content, and uneven padding on so few elements reads as a misalignment.
const UPDATE_STRIP_H: i32 = 76;
/// Clearance between the progress bar and the bottom edge.
const UPDATE_BAR_MARGIN: i32 = 15;
const UPDATE_H: i32 = UPDATE_ART_H + UPDATE_STRIP_H;

const TITLEBAR_H: i32 = 56;

// Close button size (top-right corner). Its x/y depend on the window width, so
// they are resolved per-mode rather than baked in here.
const CLOSE_W: i32 = 32;
const CLOSE_H: i32 = 32;
const CLOSE_Y: i32 = 12;

// Buttons on the error screen, laid out bottom-right: [Open log] [Close].
const ERR_BTN_W: i32 = 110;
const ERR_BTN_H: i32 = 44;
const ERR_LOG_BTN_W: i32 = 120;

// Cancel-confirmation overlay (centered card)
const CONFIRM_W: i32 = 360;
const CONFIRM_H: i32 = 160;
const CONFIRM_BTN_W: i32 = 130;
const CONFIRM_BTN_H: i32 = 40;
const CONFIRM_BTN_GAP: i32 = 16;

/// Where everything sits, for the mode this window is running in.
///
/// The two modes are different enough in shape that sharing one set of
/// constants meant the update card inherited an install card's proportions --
/// a 500px art panel beside 580px of mostly-empty text column. Resolving the
/// geometry once at startup lets each mode be laid out for what it actually
/// shows, while the paint and hit-test code stays common.
#[derive(Clone, Copy)]
struct Layout {
    w: i32,
    h: i32,
    /// Width of the splash art panel. In update mode this is the whole window.
    panel_w: i32,
    /// Left edge of the text/progress content.
    content_x: i32,
    /// Width available to that content.
    content_w: i32,
    close_x: i32,
    confirm_x: i32,
    confirm_y: i32,
    confirm_yes_x: i32,
    confirm_no_x: i32,
    confirm_btn_y: i32,
}

impl Layout {
    fn new(updating: bool) -> Self {
        let (w, h, panel_w, content_x, content_w) = if updating {
            // Content spans the strip beneath the art, inset by a small margin.
            (UPDATE_W, UPDATE_H, UPDATE_W, 20, UPDATE_W - 40)
        } else {
            (
                INSTALL_W,
                INSTALL_H,
                PANEL_W,
                PANEL_W + 56,
                INSTALL_W - PANEL_W - 112,
            )
        };

        let confirm_x = (w - CONFIRM_W) / 2;
        let confirm_y = (h - CONFIRM_H) / 2;
        let confirm_yes_x = confirm_x + CONFIRM_W - 24 - CONFIRM_BTN_W;

        Self {
            w,
            h,
            panel_w,
            content_x,
            content_w,
            close_x: w - 44,
            confirm_x,
            confirm_y,
            confirm_yes_x,
            confirm_no_x: confirm_yes_x - CONFIRM_BTN_GAP - CONFIRM_BTN_W,
            confirm_btn_y: confirm_y + CONFIRM_H - 24 - CONFIRM_BTN_H,
        }
    }
}

struct AppState {
    tx: Sender<DownloadMsg>,
    rx: std::sync::mpsc::Receiver<DownloadMsg>,
    layout: Layout,
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
    hover_log_btn: bool,
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

    let layout = Layout::new(updating);

    let app = Box::new(AppState {
        tx,
        rx: unified_rx,
        layout,
        progress: 0.0,
        // Phase 1 of the watch: the old app is still shutting down and NSIS has
        // not started writing yet. "Preparing update" names a step that ends,
        // where a bare "Updating" only repeats the window title and leaves the
        // later switch to "Installing" looking like the first real progress.
        status: if updating {
            "Preparing update".into()
        } else {
            "Downloading GitWyrm...".into()
        },
        // The compact update card has one line for the status and no room for a
        // second; the reassurance that used to live here is the status line now.
        detail: String::new(),
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
        hover_log_btn: false,
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
            (screen_w - layout.w) / 2,
            (screen_h - layout.h) / 2,
            layout.w,
            layout.h,
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

/// How long `--updating --dry-run` holds the card before closing it.
///
/// This is a UI workbench, not a simulation: the point is to have the window
/// stay put long enough to iterate on it, so it deliberately outlasts the few
/// seconds a real handover takes.
const DRY_RUN_UPDATE_HOLD: std::time::Duration = std::time::Duration::from_secs(120);

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
            // Long enough to actually look at the card and tweak it, rather than
            // the few seconds a real handover takes. `--fail` drives the error
            // state instead, which is otherwise only reachable by breaking a
            // real update.
            if std::env::args().any(|a| a == "--fail") {
                std::thread::sleep(std::time::Duration::from_secs(3));
                let _ = tx.send(DownloadMsg::Error(
                    "The update is taking longer than expected.\n\nIt may still finish on its own. \
                     If GitWyrm does not reopen, launch it from the Start Menu."
                        .into(),
                ));
                return;
            }
            // Walk the same caption change a real update goes through, so the
            // workbench shows both phases rather than only the first.
            std::thread::sleep(std::time::Duration::from_secs(4));
            let _ = tx.send(DownloadMsg::Status("Installing the new version".into()));
            std::thread::sleep(DRY_RUN_UPDATE_HOLD);
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

        // Phase 2 is the long one -- NSIS rewriting 400-odd files -- so it gets
        // its own caption. Leaving "Updating GitWyrm" up for the whole wait made
        // the slowest part of the update look like nothing was happening.
        let _ = tx.send(DownloadMsg::Status("Installing the new version".into()));

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

        match crate::run_installer(&path, &["/S"]) {
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
                crate::log(&format!("ERROR: Failed to run installer: {}", e));
                // The lock has already been waited out and is still there, so
                // the raw Windows wording would only name a process the user
                // cannot see. Say what to do about it instead.
                let msg = if crate::is_file_locked(&e) {
                    "Another program on this PC is holding the setup file open, so it could not start. Antivirus software is the usual reason. Wait a moment and run setup again.".to_string()
                } else {
                    format!("Setup could not start the installer: {}", e)
                };
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

/// Top-left corner of the error screen's Close button.
///
/// Bottom-right, where the action that dismisses a dialog is expected. One
/// definition for both the paint and the hit test: they sat apart before, which
/// is exactly how a button ends up drawn somewhere it cannot be clicked.
fn error_button_rect(s: &AppState) -> (i32, i32) {
    let l = s.layout;
    let btn_y = if s.updating {
        l.h - 16 - ERR_BTN_H
    } else {
        l.h - 56 - ERR_BTN_H
    };
    let right_edge = l.content_x + l.content_w;
    (right_edge - ERR_BTN_W, btn_y)
}

/// Top-left corner of the "Open log" button, at the left edge of the content.
///
/// Pushed to the opposite end from Close rather than paired beside it: the two
/// are not a set of alternatives to weigh up, and separating them puts the
/// destination-changing action well away from the one the user will reach for.
///
/// A failed update leaves the user with nothing to act on -- the app is gone
/// and the card only says so. The log is the one artefact that might explain
/// why, and it is already being written; this makes it reachable without
/// knowing where it lives.
fn log_button_rect(s: &AppState) -> (i32, i32) {
    let (_, btn_y) = error_button_rect(s);
    (s.layout.content_x, btn_y)
}

/// The log worth showing after a failed update, most specific first.
///
/// Deliberately NOT this helper's own log by preference. In `--updating` mode
/// the helper only watches for a process, so its log says little more than the
/// error already on screen.
///
/// 1. **The install log**, written by `installer-hooks.nsh`. This covers the
///    phase the user is staring at when the cover times out, and it is the only
///    record of it -- the app has already exited by then. A log ending before
///    "Install finished successfully" is the diagnosis.
/// 2. **The app log**, where the updater's earlier failures land: download
///    errors, signature mismatches, a bad endpoint.
/// 3. **This helper's log**, for a first install that failed before either of
///    the above existed.
fn error_log_path() -> std::path::PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let logs = std::path::Path::new(&local).join("dev.gitwyrm.app").join("logs");
        for name in ["GitWyrm-Install.log", "GitWyrm.log"] {
            let candidate = logs.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    crate::log_path().clone()
}

/// Open the log in whatever the user has associated with .log files.
///
/// Best-effort: a machine with no handler for .log simply does nothing, which
/// is no worse than the button not existing.
fn open_log() {
    let path = error_log_path();
    crate::log(&format!("Opening log at {}", path.display()));

    // `explorer` is the shell's own opener and is present on every Windows
    // install, unlike `cmd /c start`, which would flash a console window.
    let _ = std::process::Command::new("explorer").arg(&path).spawn();
}

/// Draw the error screen's action row: [Open log] [Close], bottom-right.
///
/// Close carries the accent as the primary action -- it is what the user will
/// pick, and after a failed update it is the only thing that actually resolves
/// the screen. "Open log" stays quiet beside it, offered rather than urged.
unsafe fn draw_error_buttons(hdc: HDC, s: &AppState) {
    let (close_x, btn_y) = error_button_rect(s);
    let (log_x, _) = log_button_rect(s);

    // Outline rather than a grey fill. A filled secondary still reads as a
    // button competing for the click; an outline offers the log without
    // suggesting it is the thing to do next.
    let (log_border, log_text) = if s.hover_log_btn {
        (COLOR_ACCENT, COLOR_ACCENT)
    } else {
        (COLOR_BORDER, COLOR_SUBTEXT)
    };
    stroke_rounded_rect(hdc, log_x, btn_y, ERR_LOG_BTN_W, ERR_BTN_H, 8, log_border);
    draw_text_center(hdc, "Open log", log_x, btn_y, ERR_LOG_BTN_W, ERR_BTN_H, s.font_small_bold, log_text);

    // Full accent at rest so the primary action carries its own weight, and a
    // brighter step on hover -- dimming the resting state to leave room for a
    // hover made the main action look disabled.
    let close_bg = if s.hover_close_btn { COLOR_ACCENT_BRIGHT } else { COLOR_ACCENT };
    fill_rounded_rect(hdc, close_x, btn_y, ERR_BTN_W, ERR_BTN_H, 8, close_bg);
    draw_text_center(hdc, "Close", close_x, btn_y, ERR_BTN_W, ERR_BTN_H, s.font_small_bold, COLOR_ON_ACCENT);
}

/// Draw the progress bar, and the status line above it.
///
/// Shared because the two modes differ in where the bar sits, not in what it
/// looks like or what the three states (indeterminate, determinate, idle) mean.
fn draw_progress(hdc: HDC, s: &AppState, x: i32, bar_y: i32, w: i32) {
    if !s.status.is_empty() {
        draw_text(hdc, &s.status, x, bar_y - 30, w, 26, s.font_body, COLOR_TEXT);
    }
    fill_rounded_rect(hdc, x, bar_y, w, 16, 8, COLOR_BAR_BG);
    if s.installing {
        fill_indeterminate_bar(hdc, x, bar_y, w, 16, 8, COLOR_BAR_START, COLOR_BAR_END, s.anim_tick);
    } else if s.progress > 0.001 {
        let fill_w = ((w as f64) * s.progress) as i32;
        if fill_w > 0 {
            fill_gradient_bar(hdc, x, bar_y, fill_w, 16, 8, COLOR_BAR_START, COLOR_BAR_END);
        }
    }
}

/// The compact card shown while an in-place update installs.
///
/// Deliberately almost empty: the art fills the window, and a single strip
/// underneath carries the status and the bar. Someone whose app just vanished
/// mid-session needs to see that it is coming back, which the animation says on
/// its own -- a heading, a tagline and a paragraph of copy only added the
/// negative space this layout exists to remove.
unsafe fn paint_update(hdc: HDC, s: &AppState) {
    let l = s.layout;

    // Art fills the band BELOW the header, not the whole panel.
    //
    // `draw_splash` centres the scaled image on the rect it is given, so
    // drawing it across the full panel and then painting the header on top left
    // the visible art centred on a region 52px taller than the one the eye
    // sees -- the composition read as sitting high. Handing it the visible band
    // directly is what makes it look vertically centred.
    draw_splash(hdc, 0, UPDATE_HEADER_H, l.w, UPDATE_ART_H - UPDATE_HEADER_H);

    // Header band above the art, carrying the icon and wordmark.
    fill_rect(hdc, 0, 0, l.w, UPDATE_HEADER_H, COLOR_BG);
    fill_rect(hdc, 0, UPDATE_HEADER_H, l.w, 1, COLOR_DIVIDER);
    draw_logo(hdc, 20, 12, 28, 28);
    draw_wordmark_img(hdc, 56, 15, 24);

    // Status strip.
    fill_rect(hdc, 0, UPDATE_ART_H, l.w, UPDATE_STRIP_H, COLOR_PANEL);
    fill_rect(hdc, 0, UPDATE_ART_H, l.w, 1, COLOR_DIVIDER);

    if !s.error.is_empty() {
        // The error text needs more room than the strip has, so it takes over
        // the lower part of the art rather than being clipped to two lines.
        fill_rect(hdc, 0, UPDATE_ART_H - 150, l.w, 150 + UPDATE_STRIP_H, COLOR_PANEL);
        draw_text(hdc, "Update failed", l.content_x, UPDATE_ART_H - 138, l.content_w, 32, s.font_body, COLOR_TEXT);
        draw_text_wrap(hdc, &s.error, l.content_x, UPDATE_ART_H - 100, l.content_w, 100, s.font_small, COLOR_ERROR);

        draw_error_buttons(hdc, s);
        return;
    }

    draw_progress(hdc, s, l.content_x, l.h - UPDATE_BAR_MARGIN - 16, l.content_w);
}

/// The full-size welcome card shown on a first install.
unsafe fn paint_install(hdc: HDC, s: &AppState) {
    let l = s.layout;

    // Left splash panel (fills full height behind the title bar)
    draw_splash(hdc, 0, 0, l.panel_w, l.h);

    // Right content panel background
    fill_rect(hdc, l.panel_w, 0, l.w - l.panel_w, l.h, COLOR_PANEL);

    // Title bar (icon + wordmark, over the right panel)
    draw_logo(hdc, l.panel_w + 24, 12, 32, 32);
    let wordmark_w = draw_wordmark_img(hdc, l.panel_w + 68, 16, 24);
    draw_text(hdc, " Setup", l.panel_w + 68 + wordmark_w, 18, 100, 24, s.font_body, COLOR_TEXT);
    fill_rect(hdc, l.panel_w, TITLEBAR_H, l.w - l.panel_w, 1, COLOR_DIVIDER);

    // X close button (top-right)
    let x_color = if s.hover_close_x { COLOR_HOVER } else { COLOR_SUBTEXT };
    draw_text_center(hdc, "\u{00D7}", l.close_x, CLOSE_Y, CLOSE_W, CLOSE_H, s.font_body, x_color);

    if !s.error.is_empty() {
        draw_text(hdc, "Setup failed", l.content_x, 120, l.content_w, 44, s.font_title, COLOR_TEXT);
        draw_text_wrap(hdc, &s.error, l.content_x, 190, l.content_w, 240, s.font_small_bold, COLOR_ERROR);

        draw_error_buttons(hdc, s);
        return;
    }

    draw_text(hdc, "Welcome to", l.content_x, 110, l.content_w, 44, s.font_title, COLOR_TEXT);
    let wordmark_w = draw_wordmark_img(hdc, l.content_x, 156, 40);
    draw_text(hdc, " Setup", l.content_x + wordmark_w, 154, l.content_w - wordmark_w, 44, s.font_title, COLOR_TEXT);

    draw_text(hdc, "Fast. Focused. Familiar.", l.content_x, 212, l.content_w, 30, s.font_tagline, COLOR_ACCENT);

    draw_text_wrap(
        hdc,
        "GitWyrm brings a fast, familiar, and beautiful experience to your Git workflows.",
        l.content_x,
        250,
        l.content_w,
        50,
        s.font_small,
        COLOR_SUBTEXT,
    );

    fill_rect(hdc, l.content_x, 320, l.content_w, 1, COLOR_DIVIDER);

    let bar_y = l.h - 56 - 20 - 16;
    draw_progress(hdc, s, l.content_x, bar_y, l.content_w);

    if !s.detail.is_empty() {
        draw_text(hdc, &s.detail, l.content_x, bar_y + 24, l.content_w, 20, s.font_small, COLOR_SUBTEXT);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

    if ptr.is_null() {
        if msg == WM_PAINT {
            // No state yet, so the mode is unknown: fill whatever the window
            // actually is rather than guessing at one mode's dimensions.
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            fill_rect(hdc, 0, 0, rc.right, rc.bottom, COLOR_BG);
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
                    DownloadMsg::Status(text) => {
                        s.status = text;
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
            let l = s.layout;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Double-buffer
            let mem_dc = CreateCompatibleDC(Some(hdc));
            let bmp = CreateCompatibleBitmap(hdc, l.w, l.h);
            let old = SelectObject(mem_dc, bmp.into());

            fill_rect(mem_dc, 0, 0, l.w, l.h, COLOR_BG);

            if s.updating {
                paint_update(mem_dc, s);
            } else {
                paint_install(mem_dc, s);
            }

            if s.confirming_cancel {
                fill_rect(mem_dc, 0, 0, l.w, l.h, COLOR_SCRIM);
                fill_rounded_rect(mem_dc, l.confirm_x, l.confirm_y, CONFIRM_W, CONFIRM_H, 10, COLOR_PANEL);
                draw_text(mem_dc, "Cancel setup?", l.confirm_x + 24, l.confirm_y + 24, CONFIRM_W - 48, 28, s.font_body, COLOR_TEXT);
                draw_text_wrap(
                    mem_dc,
                    "GitWyrm has not finished installing yet.",
                    l.confirm_x + 24,
                    l.confirm_y + 56,
                    CONFIRM_W - 48,
                    40,
                    s.font_small,
                    COLOR_SUBTEXT,
                );

                let no_bg = if s.hover_confirm_no { COLOR_HOVER } else { COLOR_BAR_BG };
                fill_rounded_rect(mem_dc, l.confirm_no_x, l.confirm_btn_y, CONFIRM_BTN_W, CONFIRM_BTN_H, 8, no_bg);
                draw_text_center(mem_dc, "Keep going", l.confirm_no_x, l.confirm_btn_y, CONFIRM_BTN_W, CONFIRM_BTN_H, s.font_small_bold, COLOR_TEXT);

                let yes_bg = if s.hover_confirm_yes { COLOR_ERROR } else { COLOR_BAR_BG };
                fill_rounded_rect(mem_dc, l.confirm_yes_x, l.confirm_btn_y, CONFIRM_BTN_W, CONFIRM_BTN_H, 8, yes_bg);
                draw_text_center(mem_dc, "Cancel setup", l.confirm_yes_x, l.confirm_btn_y, CONFIRM_BTN_W, CONFIRM_BTN_H, s.font_small_bold, COLOR_TEXT);
            }

            let _ = BitBlt(hdc, 0, 0, l.w, l.h, Some(mem_dc), 0, 0, SRCCOPY);

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
            let l = s.layout;
            let click_x = (lparam.0 & 0xFFFF) as i16 as i32;
            let click_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            if s.confirming_cancel {
                if click_x >= l.confirm_yes_x
                    && click_x < l.confirm_yes_x + CONFIRM_BTN_W
                    && click_y >= l.confirm_btn_y
                    && click_y < l.confirm_btn_y + CONFIRM_BTN_H
                {
                    s.exiting = true;
                    KillTimer(Some(hwnd), 1).ok();
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }

                if click_x >= l.confirm_no_x
                    && click_x < l.confirm_no_x + CONFIRM_BTN_W
                    && click_y >= l.confirm_btn_y
                    && click_y < l.confirm_btn_y + CONFIRM_BTN_H
                {
                    s.confirming_cancel = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }

                // Swallow all other clicks while the overlay is up (modal).
                return LRESULT(0);
            }

            // The update card has no close button: there is nothing to cancel
            // -- the installer is already running in another process, and this
            // window is only a cover over the gap it leaves.
            if !s.updating
                && click_x >= l.close_x
                && click_x < l.close_x + CLOSE_W
                && click_y >= CLOSE_Y
                && click_y < CLOSE_Y + CLOSE_H
            {
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
                let (log_x, log_y) = log_button_rect(s);
                if click_x >= log_x
                    && click_x < log_x + ERR_LOG_BTN_W
                    && click_y >= log_y
                    && click_y < log_y + ERR_BTN_H
                {
                    // Deliberately does not close the card: the user may want to
                    // read the log and still have the message to compare against.
                    open_log();
                    return LRESULT(0);
                }

                let (btn_x, btn_y) = error_button_rect(s);
                if click_x >= btn_x
                    && click_x < btn_x + ERR_BTN_W
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
            let l = s.layout;
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
                let over_yes = mx >= l.confirm_yes_x
                    && mx < l.confirm_yes_x + CONFIRM_BTN_W
                    && my >= l.confirm_btn_y
                    && my < l.confirm_btn_y + CONFIRM_BTN_H;
                let over_no = mx >= l.confirm_no_x
                    && mx < l.confirm_no_x + CONFIRM_BTN_W
                    && my >= l.confirm_btn_y
                    && my < l.confirm_btn_y + CONFIRM_BTN_H;

                if over_yes != s.hover_confirm_yes || over_no != s.hover_confirm_no {
                    s.hover_confirm_yes = over_yes;
                    s.hover_confirm_no = over_no;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }

                return LRESULT(0);
            }

            let over_x = !s.updating
                && mx >= l.close_x
                && mx < l.close_x + CLOSE_W
                && my >= CLOSE_Y
                && my < CLOSE_Y + CLOSE_H;
            let (over_btn, over_log) = if !s.error.is_empty() {
                let (btn_x, btn_y) = error_button_rect(s);
                let (log_x, log_y) = log_button_rect(s);
                (
                    mx >= btn_x && mx < btn_x + ERR_BTN_W && my >= btn_y && my < btn_y + ERR_BTN_H,
                    mx >= log_x && mx < log_x + ERR_LOG_BTN_W && my >= log_y && my < log_y + ERR_BTN_H,
                )
            } else {
                (false, false)
            };

            if over_x != s.hover_close_x || over_btn != s.hover_close_btn || over_log != s.hover_log_btn {
                s.hover_close_x = over_x;
                s.hover_close_btn = over_btn;
                s.hover_log_btn = over_log;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            LRESULT(0)
        }

        WM_MOUSELEAVE_MSG => {
            s.tracking_mouse = false;
            if s.hover_close_x || s.hover_close_btn || s.hover_log_btn || s.hover_confirm_yes || s.hover_confirm_no {
                s.hover_close_x = false;
                s.hover_close_btn = false;
                s.hover_log_btn = false;
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
            let l = s.layout;
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let local_x = x - rect.left;
            let local_y = y - rect.top;

            // The update card has no title bar to grab, so the art itself is the
            // drag handle -- otherwise a window with no chrome could not be moved
            // off whatever it happens to be covering.
            let draggable = if s.updating {
                local_y >= 0 && local_y < UPDATE_ART_H && local_x >= 0 && local_x < l.w
            } else {
                let over_close = local_x >= l.close_x
                    && local_x < l.close_x + CLOSE_W
                    && local_y >= CLOSE_Y
                    && local_y < CLOSE_Y + CLOSE_H;
                !over_close && local_y >= 0 && local_y < TITLEBAR_H && local_x >= l.panel_w && local_x < l.w
            };

            if draggable {
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

    /// The update card exists to be small. If it ever grows back toward the
    /// install card's proportions, the redesign has been undone by accident.
    #[test]
    fn the_update_card_is_much_smaller_than_the_install_card() {
        let update = Layout::new(true);
        let install = Layout::new(false);

        assert!(
            update.w < install.w / 2 && update.h < install.h,
            "update card {}x{} should be far smaller than the install card {}x{}",
            update.w,
            update.h,
            install.w,
            install.h
        );
    }

    /// Every piece of the update card has to fit inside it. The strip is only
    /// 92px tall, so a caption or bar drawn from the wrong origin lands off the
    /// bottom edge and simply is not painted -- no error, just a missing bar.
    #[test]
    fn update_content_fits_inside_the_window() {
        let l = Layout::new(true);

        assert_eq!(l.w, UPDATE_W);
        assert_eq!(l.h, UPDATE_ART_H + UPDATE_STRIP_H);

        // The header is drawn over the art, so it must not exceed it.
        assert!(
            UPDATE_HEADER_H < UPDATE_ART_H,
            "the wordmark header must sit inside the art panel"
        );

        // Status text, then the bar, then the bottom margin -- the layout
        // paint_update actually draws.
        let bar_y = l.h - UPDATE_BAR_MARGIN - 16;
        assert!(
            bar_y + 16 <= l.h,
            "the progress bar runs past the bottom of the window"
        );
        assert!(
            bar_y - 30 > UPDATE_ART_H,
            "the status line overlaps the art instead of sitting in the strip"
        );

        // Side insets are symmetric, so the strip reads as a padded panel.
        assert_eq!(
            l.content_x,
            l.w - (l.content_x + l.content_w),
            "content should be inset equally on both sides"
        );
    }

    /// Build just enough state to ask where the error buttons land.
    fn layout_probe(updating: bool) -> AppState {
        let (tx, rx) = std::sync::mpsc::channel();
        AppState {
            tx,
            rx,
            layout: Layout::new(updating),
            progress: 0.0,
            status: String::new(),
            detail: String::new(),
            error: "x".into(),
            font_title: HFONT::default(),
            font_tagline: HFONT::default(),
            font_body: HFONT::default(),
            font_small: HFONT::default(),
            font_small_bold: HFONT::default(),
            exiting: false,
            dry_run: true,
            updating,
            installing: false,
            anim_tick: 0,
            hover_close_x: false,
            hover_close_btn: false,
            hover_log_btn: false,
            confirming_cancel: false,
            hover_confirm_yes: false,
            hover_confirm_no: false,
            tracking_mouse: false,
        }
    }

    /// Both error buttons must fit the content column without overlapping.
    ///
    /// The update card is only 350px wide, so the two-button row is a genuine
    /// fit rather than a formality -- and an overlap would put the Close hit
    /// rect on top of "Open log", making the log unreachable.
    #[test]
    fn error_buttons_fit_side_by_side_in_both_modes() {
        for updating in [true, false] {
            let s = layout_probe(updating);
            let l = s.layout;
            let (close_x, close_y) = error_button_rect(&s);
            let (log_x, log_y) = log_button_rect(&s);

            assert_eq!(close_y, log_y, "the buttons should share a baseline");

            assert!(
                log_x >= l.content_x,
                "updating={updating}: Open log starts at {log_x}, left of the content edge {}",
                l.content_x
            );
            assert!(
                log_x + ERR_LOG_BTN_W <= close_x,
                "updating={updating}: Open log overlaps Close"
            );
            assert!(
                close_x + ERR_BTN_W <= l.content_x + l.content_w,
                "updating={updating}: Close runs past the content edge"
            );
            assert!(
                close_y + ERR_BTN_H <= l.h,
                "updating={updating}: the button row runs past the bottom of the window"
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
