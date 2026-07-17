use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git_short_hash() -> Option<String> {
  let out = Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .output()
    .ok()?;
  if !out.status.success() {
    return None;
  }
  let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
  (!hash.is_empty()).then_some(hash)
}

/// Days-since-epoch to Y-M-D without pulling in a date crate.
fn build_date() -> String {
  let days = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() / 86_400)
    .unwrap_or(0);
  let mut y = 1970i64;
  let mut d = days as i64;
  loop {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let len = if leap { 366 } else { 365 };
    if d < len {
      let months = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
      } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
      };
      let mut m = 0;
      while d >= months[m] {
        d -= months[m];
        m += 1;
      }
      return format!("{y}-{:02}-{:02}", m + 1, d + 1);
    }
    d -= len;
    y += 1;
  }
}

fn main() {
  // Build metadata surfaced in the About screen.
  println!("cargo:rustc-env=GW_BUILD_DATE={}", build_date());
  println!(
    "cargo:rustc-env=GW_GIT_HASH={}",
    git_short_hash().unwrap_or_else(|| "dev".to_string())
  );
  println!("cargo:rerun-if-changed=../.git/HEAD");

  tauri_build::build()
}
