//! One log file per calendar day, and keeping the log folder bounded.
//!
//! The log plugin only rotates on size, which produced a handful of
//! `gitwyrm_<timestamp>.log` files whose boundaries had nothing to do with when
//! anything happened -- "what did the app do on Tuesday" meant opening every one
//! of them. So GitWyrm writes the file itself: `gitwyrm-YYYY-MM-DD.log`, rolled
//! when the calendar date changes even if the app has been left running.
//!
//! A day is still capped in size, because a failing operation in a retry loop
//! can write faster than anyone reads. Past the cap the day continues in
//! `gitwyrm-YYYY-MM-DD-2.log`, and so on.
//!
//! Old days are dropped on two rules, whichever bites first: anything older than
//! [`RETENTION_DAYS`], then oldest-first until the folder fits in
//! [`MAX_FOLDER_BYTES`]. The sweep runs at startup and again every time the
//! writer opens a new file, so a long session cannot outgrow the cap.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use tauri::Manager;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use crate::error::AppError;

/// Every file this module owns starts with this, which is also what marks a file
/// in the log folder as ours to delete.
pub const LOG_FILE_PREFIX: &str = "gitwyrm";

/// Panics are written here directly rather than through the logger, which can be
/// mid-write when the process is going down.
pub const PANIC_FILE_NAME: &str = "gitwyrm-panic.log";

const DATE_FORMAT: &[FormatItem<'_>] = format_description!("[year]-[month]-[day]");

/// How many days of logs to keep. Long enough that a bug reported "last week"
/// still has the day it happened on.
pub const RETENTION_DAYS: i32 = 14;

/// How large the log folder may get before the oldest days are dropped, whatever
/// the retention window says.
pub const MAX_FOLDER_BYTES: u64 = 50_000_000;

/// How large one file may get before the day continues in a new part.
const MAX_PART_BYTES: u64 = 10_000_000;

/// How often the writer re-reads the calendar date. Checking on every record
/// would resolve the local UTC offset thousands of times a minute for a boundary
/// that moves once a day.
const DAY_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// The file the logger currently has open.
///
/// Readers need the name, and only the writer knows it: it depends on today's
/// date and on how many parts the day has already filled. Kept here rather than
/// recomputed, so a reader and the logger can never disagree about which file is
/// live.
static ACTIVE_PATH: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The log file being written to, once logging has started.
pub fn active_path() -> Option<PathBuf> {
  ACTIVE_PATH.read().ok().and_then(|slot| slot.clone())
}

fn set_active_path(path: &Path) {
  if let Ok(mut slot) = ACTIVE_PATH.write() {
    *slot = Some(path.to_path_buf());
  }
}

/// Today in the user's timezone, matching the timestamps written inside the file.
/// Falls back to UTC on the platforms where the local offset cannot be resolved.
fn today() -> Date {
  OffsetDateTime::now_local()
    .unwrap_or_else(|_| OffsetDateTime::now_utc())
    .date()
}

fn file_name_for(day: Date, part: u32) -> String {
  let date = day
    .format(DATE_FORMAT)
    .unwrap_or_else(|_| "unknown".to_string());
  if part <= 1 {
    format!("{LOG_FILE_PREFIX}-{date}.log")
  } else {
    format!("{LOG_FILE_PREFIX}-{date}-{part}.log")
  }
}

/// The day and part a log file name describes, or `None` for anything that is
/// not one of ours -- the panic log, and the `gitwyrm_<timestamp>.log` files a
/// pre-daily version left behind.
fn parse_file_name(name: &str) -> Option<(Date, u32)> {
  let rest = name
    .strip_prefix(LOG_FILE_PREFIX)?
    .strip_prefix('-')?
    .strip_suffix(".log")?;
  if rest.len() < 10 {
    return None;
  }
  let (date, tail) = rest.split_at(10);
  let day = Date::parse(date, DATE_FORMAT).ok()?;
  let part = match tail {
    "" => 1,
    _ => tail.strip_prefix('-')?.parse().ok()?,
  };
  Some((day, part))
}

/// Whether a file name is one the log folder owns. Used to keep `read_log_file`
/// from being talked into reading something else.
pub fn is_managed_name(name: &str) -> bool {
  !name.contains('/')
    && !name.contains('\\')
    && name.starts_with(LOG_FILE_PREFIX)
    && name.ends_with(".log")
}

/// The folder the app writes logs to, created if missing.
pub fn log_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
  let dir = app
    .path()
    .app_log_dir()
    .map_err(|e| AppError::Other(e.to_string()))?;
  fs::create_dir_all(&dir)?;
  Ok(dir)
}

/// Where a panic should record itself: alongside the log the app is writing, or
/// next to the executable when the logger never got far enough to open one.
pub fn panic_log_path() -> PathBuf {
  active_path()
    .and_then(|path| path.parent().map(|dir| dir.join(PANIC_FILE_NAME)))
    .unwrap_or_else(|| PathBuf::from(PANIC_FILE_NAME))
}

/// A log file in the folder, with everything the sweep needs to rank it.
pub struct LogEntry {
  pub path: PathBuf,
  pub size: u64,
  /// Days since the epoch. Taken from the name when it carries a date, and from
  /// the modification time for the older files that do not.
  day: i32,
  /// Position within the day; 0 for files whose name carries no part.
  part: u32,
  /// Whether this file's name is `gitwyrm-YYYY-MM-DD[-N].log`.
  dated: bool,
}

/// Every log file in `dir`, oldest first, skipping `except`.
pub fn entries(dir: &Path, except: Option<&Path>) -> Vec<LogEntry> {
  let Ok(read) = fs::read_dir(dir) else {
    return Vec::new();
  };
  let mut out = Vec::new();
  for entry in read.flatten() {
    let path = entry.path();
    if except.is_some_and(|skip| skip == path.as_path()) {
      continue;
    }
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
      continue;
    };
    if !is_managed_name(&name) {
      continue;
    }
    let Ok(meta) = entry.metadata() else {
      continue;
    };
    if !meta.is_file() {
      continue;
    }
    let (day, part, dated) = match parse_file_name(&name) {
      Some((date, part)) => (date.to_julian_day(), part, true),
      None => (modified_day(&meta), 0, false),
    };
    out.push(LogEntry {
      path,
      size: meta.len(),
      day,
      part,
      dated,
    });
  }
  out.sort_by(|a, b| {
    a.day
      .cmp(&b.day)
      .then(a.part.cmp(&b.part))
      .then(a.path.cmp(&b.path))
  });
  out
}

fn modified_day(meta: &fs::Metadata) -> i32 {
  meta
    .modified()
    .map(|when| OffsetDateTime::from(when).date().to_julian_day())
    .unwrap_or(0)
}

/// What a sweep removed.
pub struct Swept {
  pub removed: usize,
  pub freed: u64,
}

/// Drop expired log files, then the oldest survivors until the folder fits.
///
/// Takes the date and the limits as arguments so the rules are testable against
/// a temporary folder without waiting two weeks. Never touches `active`, which
/// the logger has open.
fn sweep_in(
  dir: &Path,
  today: Date,
  active: Option<&Path>,
  retention_days: i32,
  max_bytes: u64,
) -> Swept {
  let mut swept = Swept {
    removed: 0,
    freed: 0,
  };
  let cutoff = today.to_julian_day() - retention_days;
  let mut files = entries(dir, active);

  files.retain(|file| {
    if file.day > cutoff {
      return true;
    }
    match fs::remove_file(&file.path) {
      Ok(()) => {
        swept.removed += 1;
        swept.freed += file.size;
        false
      }
      // A file we could not delete still counts against the size cap, so leave
      // it in the list rather than pretending it is gone.
      Err(_) => true,
    }
  });

  let active_size = active
    .and_then(|path| fs::metadata(path).ok())
    .map(|meta| meta.len())
    .unwrap_or(0);
  let mut total = active_size + files.iter().map(|file| file.size).sum::<u64>();
  for file in &files {
    if total <= max_bytes {
      break;
    }
    if fs::remove_file(&file.path).is_ok() {
      total = total.saturating_sub(file.size);
      swept.removed += 1;
      swept.freed += file.size;
    }
  }

  swept
}

/// Apply the retention rules to the app's log folder and say what went.
pub fn sweep(app: &tauri::AppHandle) {
  let Ok(dir) = log_dir(app) else {
    return;
  };
  let active = active_path();
  let swept = sweep_in(
    &dir,
    today(),
    active.as_deref(),
    RETENTION_DAYS,
    MAX_FOLDER_BYTES,
  );
  if swept.removed > 0 {
    log::info!(
      "Removed {} old log file(s), freeing {} KB",
      swept.removed,
      swept.freed / 1024
    );
  }
}

/// The tail of the log across day boundaries, newest content last.
///
/// Reports want the last few hours, and a report filed just after midnight would
/// otherwise carry a nearly empty file. Walks back through the daily files until
/// it has `max_bytes` or runs out.
pub fn recent_text(dir: &Path, max_bytes: usize) -> String {
  let files = entries(dir, None);
  let mut budget = max_bytes;
  let mut chunks: Vec<String> = Vec::new();
  for file in files.iter().rev().filter(|file| file.dated) {
    if budget == 0 {
      break;
    }
    let Ok(text) = fs::read_to_string(&file.path) else {
      continue;
    };
    let chunk = tail_from_line_boundary(&text, budget);
    budget = budget.saturating_sub(chunk.len());
    chunks.push(chunk);
  }
  chunks.reverse();
  chunks.concat()
}

/// Keep the last `max_bytes` of `text`, starting at the next line boundary so
/// the first entry is never a fragment. Returns the whole input when it fits.
pub fn tail_from_line_boundary(text: &str, max_bytes: usize) -> String {
  if text.len() <= max_bytes {
    return text.to_string();
  }
  // Slice on a char boundary first: the cut point can land mid-UTF-8, and
  // indexing a &str there panics.
  let mut start = text.len() - max_bytes;
  while start < text.len() && !text.is_char_boundary(start) {
    start += 1;
  }
  let cut = &text[start..];
  match cut.find('\n') {
    Some(nl) => cut[nl + 1..].to_string(),
    None => cut.to_string(),
  }
}

/// Which part of `day` a new writer should continue in: the newest one that
/// still has room, or a fresh one past it.
fn resume_part(dir: &Path, day: Date) -> u32 {
  let highest = entries(dir, None)
    .iter()
    .filter(|file| file.dated && file.day == day.to_julian_day())
    .map(|file| file.part)
    .max()
    .unwrap_or(0);
  if highest == 0 {
    return 1;
  }
  let full = fs::metadata(dir.join(file_name_for(day, highest)))
    .map(|meta| meta.len() >= MAX_PART_BYTES)
    .unwrap_or(false);
  if full {
    highest + 1
  } else {
    highest
  }
}

/// The log file the app appends to, rolled on the date and on size.
///
/// Buffers in `write` and does the real work in `flush`, because that is the
/// contract fern's writer output uses: it writes one formatted record then
/// flushes, so a roll decided in `flush` never splits a line across two files.
struct DailyLogFile {
  dir: PathBuf,
  day: Date,
  part: u32,
  file: Option<File>,
  size: u64,
  buffer: Vec<u8>,
  next_day_check: Instant,
}

impl DailyLogFile {
  fn new(dir: PathBuf) -> io::Result<Self> {
    fs::create_dir_all(&dir)?;
    let day = today();
    let part = resume_part(&dir, day);
    let mut writer = Self {
      dir,
      day,
      part,
      file: None,
      size: 0,
      buffer: Vec::new(),
      next_day_check: Instant::now() + DAY_CHECK_INTERVAL,
    };
    writer.open()?;
    Ok(writer)
  }

  fn open(&mut self) -> io::Result<()> {
    let path = self.dir.join(file_name_for(self.day, self.part));
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    self.size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    self.file = Some(file);
    set_active_path(&path);
    Ok(())
  }

  /// Continue in a new file and trim the folder behind us.
  ///
  /// Deliberately silent: this runs inside the logger, and fern holds the lock
  /// on this writer for the whole record, so a `log::` call here would wait on a
  /// lock its own thread already owns.
  fn roll(&mut self, day: Date, part: u32) -> io::Result<()> {
    if let Some(mut file) = self.file.take() {
      let _ = file.flush();
    }
    self.day = day;
    self.part = part;
    self.open()?;
    let active = self.dir.join(file_name_for(self.day, self.part));
    let _ = sweep_in(
      &self.dir,
      self.day,
      Some(&active),
      RETENTION_DAYS,
      MAX_FOLDER_BYTES,
    );
    Ok(())
  }
}

impl Write for DailyLogFile {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.buffer.extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    if self.buffer.is_empty() {
      return Ok(());
    }

    // The date first: a session left open overnight belongs in the new day.
    if Instant::now() >= self.next_day_check {
      self.next_day_check = Instant::now() + DAY_CHECK_INTERVAL;
      let now = today();
      if now != self.day {
        self.roll(now, 1)?;
      }
    }

    if self.file.is_none() {
      self.open()?;
    }
    if self.size > 0 && self.size + self.buffer.len() as u64 > MAX_PART_BYTES {
      // "Clear logs" truncates this file underneath us, so confirm against disk
      // before starting a new part -- otherwise clearing a nearly full day would
      // be followed by an immediate roll into an empty one.
      self.size = self
        .file
        .as_ref()
        .and_then(|file| file.metadata().ok())
        .map(|meta| meta.len())
        .unwrap_or(self.size);
      if self.size + self.buffer.len() as u64 > MAX_PART_BYTES {
        let (day, part) = (self.day, self.part + 1);
        self.roll(day, part)?;
      }
    }

    if let Some(file) = self.file.as_mut() {
      file.write_all(&self.buffer)?;
      self.size += self.buffer.len() as u64;
      file.flush()?;
    }
    self.buffer.clear();
    Ok(())
  }
}

/// The log plugin target that writes the daily file.
///
/// A `Dispatch` target rather than `LogDir`, because `LogDir` comes with the
/// plugin's own size-only rotation and no way to replace it. Records still
/// arrive formatted by the plugin, so the file's contents are unchanged.
pub fn file_target(app: &tauri::AppHandle) -> Result<tauri_plugin_log::Target, AppError> {
  let dir = log_dir(app)?;
  let writer = DailyLogFile::new(dir)?;
  Ok(tauri_plugin_log::Target::new(
    tauri_plugin_log::TargetKind::Dispatch(
      fern::Dispatch::new().chain(fern::Output::writer(Box::new(writer), "\n")),
    ),
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use time::macros::date;

  fn write_log(dir: &Path, name: &str, bytes: usize) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "x".repeat(bytes)).expect("write log file");
    path
  }

  #[test]
  fn file_names_carry_the_day_and_the_part() {
    let day = date!(2026 - 08 - 13);
    assert_eq!(file_name_for(day, 1), "gitwyrm-2026-08-13.log");
    assert_eq!(file_name_for(day, 3), "gitwyrm-2026-08-13-3.log");
    assert_eq!(parse_file_name("gitwyrm-2026-08-13.log"), Some((day, 1)));
    assert_eq!(parse_file_name("gitwyrm-2026-08-13-3.log"), Some((day, 3)));
  }

  #[test]
  fn other_files_in_the_folder_are_not_daily_logs() {
    // The panic log and the files a pre-daily version rotated. Both are still
    // ours to clean up, but neither carries a day in its name.
    assert_eq!(parse_file_name("gitwyrm-panic.log"), None);
    assert_eq!(parse_file_name("gitwyrm_2026-08-13_10-00-00.log"), None);
    assert_eq!(parse_file_name("gitwyrm.log"), None);
    assert!(is_managed_name("gitwyrm-panic.log"));
    assert!(is_managed_name("gitwyrm.log"));
    assert!(!is_managed_name("something-else.log"));
    assert!(!is_managed_name("gitwyrm-2026-08-13.txt"));
  }

  #[test]
  fn a_traversal_attempt_is_not_a_managed_name() {
    assert!(!is_managed_name("../gitwyrm-2026-08-13.log"));
    assert!(!is_managed_name("gitwyrm/../../secrets.log"));
  }

  #[test]
  fn days_past_the_window_are_removed() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let old = write_log(dir.path(), "gitwyrm-2026-07-01.log", 10);
    let recent = write_log(dir.path(), "gitwyrm-2026-08-12.log", 10);
    let active = write_log(dir.path(), "gitwyrm-2026-08-13.log", 10);

    let swept = sweep_in(
      dir.path(),
      date!(2026 - 08 - 13),
      Some(&active),
      14,
      MAX_FOLDER_BYTES,
    );

    assert_eq!(swept.removed, 1);
    assert!(!old.exists());
    assert!(recent.exists());
    assert!(active.exists());
  }

  #[test]
  fn the_oldest_days_go_when_the_folder_is_too_big() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let oldest = write_log(dir.path(), "gitwyrm-2026-08-10.log", 400);
    let middle = write_log(dir.path(), "gitwyrm-2026-08-11.log", 400);
    let newest = write_log(dir.path(), "gitwyrm-2026-08-12.log", 400);
    let active = write_log(dir.path(), "gitwyrm-2026-08-13.log", 400);

    // Room for the active file plus one other.
    let swept = sweep_in(dir.path(), date!(2026 - 08 - 13), Some(&active), 14, 900);

    assert_eq!(swept.removed, 2);
    assert!(!oldest.exists());
    assert!(!middle.exists());
    assert!(newest.exists());
    assert!(active.exists());
  }

  #[test]
  fn the_active_file_is_never_deleted() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let active = write_log(dir.path(), "gitwyrm-2026-08-13.log", 5_000);

    // Both rules would take it: far past the window, and over the size cap on
    // its own.
    let swept = sweep_in(dir.path(), date!(2030 - 01 - 01), Some(&active), 1, 10);

    assert_eq!(swept.removed, 0);
    assert!(active.exists());
  }

  #[test]
  fn files_from_an_older_version_are_cleaned_up_too() {
    let dir = tempfile::tempdir().expect("temporary directory");
    // No date in the name, so these age by modification time -- which for a file
    // written just now is today, keeping them until the window passes.
    let legacy = write_log(dir.path(), "gitwyrm.log", 400);
    let rotated = write_log(dir.path(), "gitwyrm_2026-08-01_09-00-00.log", 400);
    let unrelated = write_log(dir.path(), "other-app.log", 400);
    let active = write_log(dir.path(), "gitwyrm-2026-08-13.log", 10);

    // Squeeze the folder so the size rule has to act.
    let swept = sweep_in(dir.path(), date!(2026 - 08 - 13), Some(&active), 14, 100);

    assert_eq!(swept.removed, 2);
    assert!(!legacy.exists());
    assert!(!rotated.exists());
    assert!(unrelated.exists(), "another app's log is not ours to delete");
    assert!(active.exists());
  }

  #[test]
  fn a_new_writer_continues_the_days_newest_part() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let day = date!(2026 - 08 - 13);
    assert_eq!(resume_part(dir.path(), day), 1, "nothing written yet");

    write_log(dir.path(), "gitwyrm-2026-08-13.log", 10);
    assert_eq!(resume_part(dir.path(), day), 1, "room left in part one");

    write_log(
      dir.path(),
      "gitwyrm-2026-08-13.log",
      MAX_PART_BYTES as usize + 1,
    );
    assert_eq!(resume_part(dir.path(), day), 2, "part one is full");

    write_log(dir.path(), "gitwyrm-2026-08-13-2.log", 10);
    assert_eq!(resume_part(dir.path(), day), 2);
  }

  #[test]
  fn the_writer_rolls_when_the_day_changes() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let mut writer = DailyLogFile::new(dir.path().to_path_buf()).expect("open writer");
    writer.write_all(b"before\n").expect("write");
    writer.flush().expect("flush");

    let first = dir.path().join(file_name_for(writer.day, writer.part));
    let next = writer.day.next_day().expect("next day");
    writer.roll(next, 1).expect("roll");
    writer.write_all(b"after\n").expect("write");
    writer.flush().expect("flush");

    let second = dir.path().join(file_name_for(next, 1));
    assert_ne!(first, second);
    assert_eq!(fs::read_to_string(&first).expect("first day"), "before\n");
    assert_eq!(fs::read_to_string(&second).expect("second day"), "after\n");
    assert_eq!(active_path().as_deref(), Some(second.as_path()));
  }

  #[test]
  fn records_logged_through_fern_reach_the_daily_file() {
    // The writer is driven by fern, which writes a record and then flushes.
    // This stands up the same chain the log plugin builds around a Dispatch
    // target, so a change to that contract fails here rather than in the app.
    let dir = tempfile::tempdir().expect("temporary directory");
    let writer = DailyLogFile::new(dir.path().to_path_buf()).expect("open writer");
    let (_level, logger) = fern::Dispatch::new()
      .level(log::LevelFilter::Debug)
      .chain(fern::Output::writer(Box::new(writer), "\n"))
      .into_log();

    logger.log(
      &log::Record::builder()
        .args(format_args!("hello from the daily file"))
        .level(log::Level::Info)
        .build(),
    );
    logger.flush();

    let path = dir.path().join(file_name_for(today(), 1));
    let text = fs::read_to_string(&path).expect("daily log file");
    assert_eq!(text, "hello from the daily file\n");
  }

  #[test]
  fn the_report_tail_reaches_back_into_earlier_days() {
    let dir = tempfile::tempdir().expect("temporary directory");
    write_log(dir.path(), "gitwyrm-panic.log", 50);
    fs::write(dir.path().join("gitwyrm-2026-08-12.log"), "yesterday\n").expect("write");
    fs::write(dir.path().join("gitwyrm-2026-08-13.log"), "today\n").expect("write");

    let text = recent_text(dir.path(), 1_000);

    assert_eq!(text, "yesterday\ntoday\n");
  }

  #[test]
  fn the_report_tail_stops_at_the_budget() {
    let dir = tempfile::tempdir().expect("temporary directory");
    fs::write(dir.path().join("gitwyrm-2026-08-12.log"), "yesterday\n").expect("write");
    fs::write(dir.path().join("gitwyrm-2026-08-13.log"), "today\n").expect("write");

    let text = recent_text(dir.path(), 6);

    assert_eq!(text, "today\n");
  }

  #[test]
  fn short_logs_are_returned_whole() {
    let text = "[INFO] one\n[INFO] two\n";
    assert_eq!(tail_from_line_boundary(text, 1000), text);
  }

  #[test]
  fn long_logs_start_on_a_whole_entry() {
    let text = "[INFO] aaaaaaaaaa\n[INFO] bbbbbbbbbb\n[INFO] cccccccccc\n";
    // Cuts inside the second line; the partial entry must be dropped.
    let out = tail_from_line_boundary(text, 25);
    assert!(out.starts_with("[INFO] "), "got {out:?}");
    assert!(out.ends_with("[INFO] cccccccccc\n"));
    assert!(!out.contains("aaaaaaaaaa"));
  }

  #[test]
  fn multibyte_text_does_not_panic_at_the_cut() {
    // A cut landing mid-UTF-8 would panic on a naive slice.
    let text = "[INFO] \u{2705}\u{2705}\u{2705}\u{2705}\u{2705}\n[INFO] done\n";
    for max in 1..text.len() {
      let out = tail_from_line_boundary(text, max);
      assert!(text.ends_with(&out) || out.is_empty(), "max={max}");
    }
  }

  #[test]
  fn a_single_huge_line_is_still_trimmed() {
    // No newline to cut on: return the tail rather than the whole line.
    let text = format!("[INFO] {}", "x".repeat(500));
    let out = tail_from_line_boundary(&text, 100);
    assert_eq!(out.len(), 100);
  }
}
