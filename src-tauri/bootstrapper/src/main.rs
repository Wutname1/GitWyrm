#![windows_subsystem = "windows"]

mod components;
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

// The installer lives under installers/latest/, not at the CDN root. The root
// holds this bootstrapper, published under the same GitWyrm-Setup.exe name that
// the website's download button points at -- pointing here at the root would make
// the bootstrapper download and run itself.
const INSTALLER_PATH: &str = "installers/latest";

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
    /// Replace the status line without changing anything else.
    ///
    /// The update watcher has two phases that take noticeably different
    /// amounts of time, and one frozen caption across both makes the longer of
    /// them look like a hang.
    Status(String),
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
    /// Cover an in-place update the running app has already kicked off.
    updating: bool,
}

fn parse_args<I: Iterator<Item = String>>(args: I) -> Options {
    let mut opts = Options {
        silent: false,
        dry_run: false,
        log: None,
        help: false,
        bad_arg: None,
        updating: false,
    };

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "/s" | "/silent" | "--silent" => opts.silent = true,
            "/norestart" => {} // accepted and ignored; setup never reboots
            "--dry-run" => opts.dry_run = true,
            // Undocumented in USAGE: the app passes this to itself, it is not
            // something a person or a deployment tool has any reason to run.
            "--updating" => opts.updating = true,
            // Development only, and only meaningful with --dry-run: forces the
            // error screen so it can be worked on without breaking an update.
            "--fail" => {}
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

    // Updating covers an install the app already started, so there is nothing
    // to fetch: the window watches for the relaunched app instead. Silent makes
    // no sense here either -- covering a gap is the entire job -- so it is
    // ignored rather than exiting immediately and leaving the gap uncovered.
    if opts.updating {
        window::run(rx);
        return;
    }

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
            // Status is a caption for a window there isn't one of here.
            Ok(DownloadMsg::Status(_)) => {}
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

    let status = run_installer(&installer, &["/S"]);
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

/// True when Windows refused to touch a file because something else has it
/// open: `ERROR_SHARING_VIOLATION`, or the `ERROR_ACCESS_DENIED` a scanner
/// holding the same file reports instead.
pub fn is_file_locked(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32) | Some(5))
}

/// Start an installer, waiting out the moment where the file it was just
/// downloaded into still counts as in use.
///
/// A freshly written .exe is locked twice over. Windows will not start an
/// executable while any writable handle to it is open, and antivirus opens
/// every new .exe to scan it the instant it lands - and the second one is
/// nobody's handle to close. Both report the same "the process cannot access
/// the file because it is being used by another process", and both clear on
/// their own within a second or two, so setup waits rather than telling the
/// user it failed.
pub fn run_installer<S: AsRef<std::ffi::OsStr>>(
    path: &std::path::Path,
    args: &[S],
) -> std::io::Result<std::process::ExitStatus> {
    /// Roughly eight seconds of waiting in total, which covers an on-access
    /// scan of an installer-sized file. Past that the lock is something setup
    /// cannot wait out, and saying so beats hanging on the progress bar.
    const ATTEMPTS: u32 = 8;
    const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

    let shown = args
        .iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    log(&format!("Running installer: {} {}", path.display(), shown));

    let mut attempt = 1;
    let mut wait = std::time::Duration::from_millis(150);

    loop {
        match std::process::Command::new(path).args(args).status() {
            Ok(status) => return Ok(status),
            Err(e) if attempt < ATTEMPTS && is_file_locked(&e) => {
                log(&format!(
                    "{} is still in use ({}); retrying in {} ms (attempt {} of {})",
                    path.display(),
                    e,
                    wait.as_millis(),
                    attempt,
                    ATTEMPTS,
                ));
                std::thread::sleep(wait);
                wait = (wait * 2).min(MAX_WAIT);
                attempt += 1;
            }
            Err(e) => return Err(e),
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

/// An HTTP client that honors the usual proxy environment variables.
///
/// reqwest is built here without default features, so it does no proxy
/// auto-detection - corporate networks need this to be explicit.
fn http_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    let mut builder = reqwest::blocking::Client::builder().user_agent("GitWyrm-Setup/1.0");

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

    builder.build()
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
    let url = format!("{}/{}/{}", CDN_BASE, INSTALLER_PATH, installer_filename());
    log(&format!("Downloading: {}", url));

    let temp_dir = std::env::temp_dir();
    let dest = temp_dir.join(format!("GitWyrm-Setup-{}.exe", std::process::id()));

    let client = match http_client() {
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

    // Closed here rather than left to the end of this function: the window
    // starts the installer the moment it sees `Done`, and Windows refuses to
    // start an executable while a writable handle to it is still open. Leaving
    // the drop implicit made that a race the download usually lost.
    if let Err(e) = file.flush() {
        let _ = tx.send(DownloadMsg::Error(format!("Write error: {}", e)));
        return;
    }
    drop(file);

    let _ = tx.send(DownloadMsg::Progress(downloaded, total_size));
    let _ = tx.send(DownloadMsg::Done(dest));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_still_in_use_is_worth_waiting_out() {
        // 32 is ERROR_SHARING_VIOLATION, the one the download handle and an
        // antivirus scan both produce; 5 is the ERROR_ACCESS_DENIED some
        // scanners report for the same file instead.
        let sharing = std::io::Error::from_raw_os_error(32);
        let denied = std::io::Error::from_raw_os_error(5);
        assert!(is_file_locked(&sharing));
        assert!(is_file_locked(&denied));
    }

    #[test]
    fn a_real_failure_is_not_mistaken_for_a_lock() {
        // 2 is ERROR_FILE_NOT_FOUND and 216 is a bad image format. Waiting does
        // not fix either, so neither may spend the retry budget.
        assert!(!is_file_locked(&std::io::Error::from_raw_os_error(2)));
        assert!(!is_file_locked(&std::io::Error::from_raw_os_error(216)));
    }

    #[test]
    fn a_missing_installer_fails_without_retrying() {
        // The retry exists for a lock that clears itself. Anything else must
        // surface at once rather than leaving the progress bar sitting there.
        let missing = std::env::temp_dir().join("gitwyrm-no-such-installer.exe");
        let _ = std::fs::remove_file(&missing);

        let started = std::time::Instant::now();
        let result = run_installer(&missing, &["/S"]);

        assert!(result.is_err(), "a missing installer cannot have run");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "took {:?}, so it retried something it should not have",
            started.elapsed()
        );
    }
}
