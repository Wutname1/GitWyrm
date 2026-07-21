#![windows_subsystem = "windows"]

mod paint;
mod window;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

pub fn log(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("GitWyrm-Setup.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let now = chrono_lite();
        let _ = writeln!(f, "[{}] {}", now, msg);
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

fn main() {
    let dry_run = std::env::args().any(|a| a == "--dry-run");

    let (tx, rx) = mpsc::channel::<DownloadMsg>();

    thread::spawn(move || {
        if dry_run {
            fake_download(tx);
        } else {
            download_installer(tx);
        }
    });

    window::run(rx);
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

    let client = match reqwest::blocking::Client::builder()
        .user_agent("GitWyrm-Setup/1.0")
        .build()
    {
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
