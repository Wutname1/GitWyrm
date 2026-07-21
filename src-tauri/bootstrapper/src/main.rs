#![windows_subsystem = "windows"]

mod paint;
mod window;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn log_path() -> &'static PathBuf {
    LOG_PATH.get_or_init(|| std::env::temp_dir().join("GitWyrm-Setup.log"))
}

pub fn set_log_path(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

pub fn log(msg: &str) {
    use std::io::Write;

    // The download and install threads both log; serialize so lines from one
    // thread never interleave into the middle of another's.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {}", chrono_lite(), msg);
    }
}

fn chrono_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

const CDN_BASE: &str = "https://cdn.gitwyrm.com";

fn installer_filename() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "GitWyrm-Setup-ARM64.exe"
    } else {
        "GitWyrm-Setup.exe"
    }
}

pub enum DownloadMsg {
    Progress(u64, u64),
    Done(PathBuf),
    Installed,
    Error(String),
}

pub const APP_EXE_NAME: &str = "GitWyrm.exe";

// Exit codes, for Intune / SCCM detection and reporting.
pub const EXIT_OK: i32 = 0;
pub const EXIT_USAGE: i32 = 1;
pub const EXIT_DOWNLOAD_FAILED: i32 = 2;
pub const EXIT_INSTALLER_FAILED: i32 = 3;

struct Options {
    silent: bool,
    dry_run: bool,
    log: Option<PathBuf>,
    help: bool,
    bad_arg: Option<String>,
}

fn parse_args<I: Iterator<Item = String>>(args: I) -> Options {
    let mut opts = Options {
        silent: false,
        dry_run: false,
        log: None,
        help: false,
        bad_arg: None,
    };

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "/s" | "/silent" | "--silent" => opts.silent = true,
            "/norestart" => {} // accepted and ignored; setup never reboots
            "--dry-run" => opts.dry_run = true,
            "/?" | "/h" | "/help" | "-h" | "--help" => opts.help = true,
            "/log" | "--log" => match args.next() {
                Some(p) => opts.log = Some(PathBuf::from(p)),
                None => opts.bad_arg = Some(format!("{} requires a file path", arg)),
            },
            _ => {
                if let Some(rest) = lower.strip_prefix("/log=").or_else(|| lower.strip_prefix("--log=")) {
                    opts.log = Some(PathBuf::from(rest));
                } else if opts.bad_arg.is_none() {
                    opts.bad_arg = Some(format!("Unknown option: {}", arg));
                }
            }
        }
    }

    opts
}

const USAGE: &str = "\
GitWyrm Setup

Usage: GitWyrm-Setup.exe [options]

  /S, /silent      Install with no user interface. Required for Intune,
                   SCCM, and other managed deployment.
  /log <path>      Write the setup log to <path> instead of %TEMP%.
  /?               Show this message.

GitWyrm installs per-user into %LOCALAPPDATA%\\GitWyrm. Deploy it in
Intune's \"user\" install context, not as SYSTEM.

Exit codes: 0 success, 1 bad usage, 2 download failed, 3 installer failed.
";

/// This is a GUI-subsystem binary, so there is no console of our own. Attach to
/// the calling console when there is one, otherwise fall back to a message box.
fn show_usage(code: i32) -> ! {
    use windows::core::HSTRING;
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS).is_ok() };
    if attached {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(USAGE.as_bytes());
        let _ = out.flush();
    } else {
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(USAGE),
                &HSTRING::from("GitWyrm Setup"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }

    std::process::exit(code);
}

fn main() {
    let opts = parse_args(std::env::args().skip(1));

    if let Some(path) = opts.log.clone() {
        set_log_path(path);
    }

    if opts.help {
        show_usage(EXIT_OK);
    }

    if let Some(bad) = opts.bad_arg.clone() {
        log(&format!("ERROR: {}", bad));
        if opts.silent {
            std::process::exit(EXIT_USAGE);
        }
        show_usage(EXIT_USAGE);
    }

    let (tx, rx) = mpsc::channel::<DownloadMsg>();

    let dry_run = opts.dry_run;
    thread::spawn(move || {
        if dry_run {
            fake_download(tx);
        } else {
            download_installer(tx);
        }
    });

    if opts.silent {
        std::process::exit(run_silent(rx, &opts));
    }

    window::run(rx);
}

/// Drives the download/install to completion with no window, returning an
/// exit code the deployment tool can act on.
fn run_silent(rx: mpsc::Receiver<DownloadMsg>, opts: &Options) -> i32 {
    log("Silent install started");

    // Log at most once per 10% so a large download does not flood the log.
    let mut last_decile = u64::MAX;

    let installer = loop {
        match rx.recv() {
            Ok(DownloadMsg::Progress(done, total)) => {
                let decile = if total > 0 { done * 10 / total } else { 0 };
                if decile != last_decile {
                    last_decile = decile;
                    log(&format!("Downloaded {} / {} bytes", done, total));
                }
            }
            Ok(DownloadMsg::Done(path)) => break path,
            Ok(DownloadMsg::Error(e)) => {
                log(&format!("ERROR: {}", e));
                return EXIT_DOWNLOAD_FAILED;
            }
            Ok(DownloadMsg::Installed) => return EXIT_OK,
            Err(e) => {
                log(&format!("ERROR: download ended unexpectedly: {}", e));
                return EXIT_DOWNLOAD_FAILED;
            }
        }
    };

    if opts.dry_run {
        log("Dry run: skipping installer execution");
        return EXIT_OK;
    }

    if !installer.exists() {
        log(&format!("ERROR: Installer not found at {}", installer.display()));
        return EXIT_INSTALLER_FAILED;
    }

    log(&format!("Running installer: {} /S", installer.display()));
    let status = std::process::Command::new(&installer).arg("/S").status();
    let _ = std::fs::remove_file(&installer);

    match status {
        Ok(status) if status.success() => {
            // A silent install never launches the app; the user starts it themselves.
            log("Install complete");
            EXIT_OK
        }
        Ok(status) => {
            log(&format!("ERROR: Installer exited with code {}", status.code().unwrap_or(-1)));
            EXIT_INSTALLER_FAILED
        }
        Err(e) => {
            log(&format!("ERROR: Failed to run installer: {}", e));
            EXIT_INSTALLER_FAILED
        }
    }
}

/// Strips `user:password@` from a proxy URL so credentials never reach the log.
fn redact_credentials(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((s, r)) => (format!("{}://", s), r),
        None => (String::new(), url),
    };
    match rest.rsplit_once('@') {
        Some((_, host)) => format!("{}***@{}", scheme, host),
        None => url.to_string(),
    }
}

fn proxy_from_env() -> Option<String> {
    for key in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn fake_download(tx: mpsc::Sender<DownloadMsg>) {
    let total: u64 = 25_000_000;
    let steps = 100;
    for i in 0..=steps {
        let downloaded = total * i / steps;
        let _ = tx.send(DownloadMsg::Progress(downloaded, total));
        thread::sleep(std::time::Duration::from_millis(100));
    }
    thread::sleep(std::time::Duration::from_millis(500));
    let _ = tx.send(DownloadMsg::Done(PathBuf::from("C:\\fake\\GitWyrm-Setup.exe")));
}

fn download_installer(tx: mpsc::Sender<DownloadMsg>) {
    let url = format!("{}/{}", CDN_BASE, installer_filename());
    log(&format!("Downloading: {}", url));

    let temp_dir = std::env::temp_dir();
    let dest = temp_dir.join(format!("GitWyrm-Setup-{}.exe", std::process::id()));

    let mut builder = reqwest::blocking::Client::builder().user_agent("GitWyrm-Setup/1.0");

    // Corporate networks often require a proxy. reqwest is built here without
    // default features, so it does no proxy auto-detection - honor the usual
    // environment variables explicitly.
    if let Some(proxy_url) = proxy_from_env() {
        match reqwest::Proxy::all(&proxy_url) {
            Ok(proxy) => {
                log(&format!("Using proxy: {}", redact_credentials(&proxy_url)));
                builder = builder.proxy(proxy);
            }
            Err(e) => log(&format!(
                "WARNING: Ignoring invalid proxy {}: {}",
                redact_credentials(&proxy_url),
                e
            )),
        }
    }

    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(DownloadMsg::Error(format!("HTTP client error: {}", e)));
            return;
        }
    };

    let response = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(DownloadMsg::Error(format!("Connection failed: {}", e)));
            return;
        }
    };

    if !response.status().is_success() {
        let _ = tx.send(DownloadMsg::Error(format!(
            "Server returned {}",
            response.status()
        )));
        return;
    }

    let total_size = response.content_length().unwrap_or(0);

    let mut file = match std::fs::File::create(&dest) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(DownloadMsg::Error(format!("Cannot write file: {}", e)));
            return;
        }
    };

    use std::io::{Read, Write};
    let mut reader = response;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65536];
    let mut last_report = std::time::Instant::now();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = file.write_all(&buf[..n]) {
                    let _ = tx.send(DownloadMsg::Error(format!("Write error: {}", e)));
                    return;
                }
                downloaded += n as u64;
                if last_report.elapsed() > std::time::Duration::from_millis(50) {
                    let _ = tx.send(DownloadMsg::Progress(downloaded, total_size));
                    last_report = std::time::Instant::now();
                }
            }
            Err(e) => {
                let _ = tx.send(DownloadMsg::Error(format!("Download error: {}", e)));
                return;
            }
        }
    }

    let _ = tx.send(DownloadMsg::Progress(downloaded, total_size));
    let _ = tx.send(DownloadMsg::Done(dest));
}
