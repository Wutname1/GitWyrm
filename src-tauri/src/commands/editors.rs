//! Detecting which code editors are installed and handing a repository (or a
//! single file) to one of them.
//!
//! Two families live here. The *editors* -- VS Code and friends -- open a
//! folder, and the user picks one of them as their default in Settings. Visual
//! Studio is different: it opens a `.sln` file rather than a folder, so it is
//! detected separately and offered per-solution alongside the chosen editor.

use std::path::Path;
// Only the Visual Studio lookups below build a PathBuf, and those are Windows-only.
#[cfg(windows)]
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

/// A folder-opening editor. Serialized as the ids the frontend stores in
/// settings, so these names are part of the saved-settings format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EditorKind {
  VsCode,
  Cursor,
  Windsurf,
  Jetbrains,
  Zed,
}

impl EditorKind {
  /// Every editor we know how to look for, in the order they are offered.
  pub const ALL: [EditorKind; 5] = [
    EditorKind::VsCode,
    EditorKind::Cursor,
    EditorKind::Windsurf,
    EditorKind::Jetbrains,
    EditorKind::Zed,
  ];

  /// Name shown on the button and in Settings.
  pub fn label(self) -> &'static str {
    match self {
      EditorKind::VsCode => "VS Code",
      EditorKind::Cursor => "Cursor",
      EditorKind::Windsurf => "Windsurf",
      EditorKind::Jetbrains => "JetBrains",
      EditorKind::Zed => "Zed",
    }
  }

  /// The launcher command names to try, best first. JetBrains ships a
  /// per-IDE launcher rather than one shared name, so it lists several.
  fn commands(self) -> &'static [&'static str] {
    match self {
      EditorKind::VsCode => &["code"],
      EditorKind::Cursor => &["cursor"],
      EditorKind::Windsurf => &["windsurf"],
      EditorKind::Jetbrains => &["rider", "idea", "pycharm", "webstorm", "phpstorm", "clion", "rubymine", "goland"],
      EditorKind::Zed => &["zed"],
    }
  }
}

/// One installed editor, as offered to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InstalledEditor {
  pub kind: EditorKind,
  /// Display name, e.g. "VS Code".
  pub label: String,
  /// The launcher that was found, e.g. "code" or "rider".
  pub command: String,
}

/// One Visual Studio solution found inside a repository.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SolutionFile {
  /// File name with extension, e.g. "MyApp.sln".
  pub name: String,
  /// Path relative to the repo root, using forward slashes.
  pub relative_path: String,
  /// Absolute path on disk, used to launch Visual Studio.
  pub absolute_path: String,
}

/// What the toolbar's open button needs to draw itself: the editors that are
/// installed, whether Visual Studio is available, and any solutions to offer.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EditorAvailability {
  pub editors: Vec<InstalledEditor>,
  /// Display name of the detected Visual Studio, e.g. "Visual Studio 2022".
  /// None when Visual Studio is not installed.
  pub visual_studio: Option<String>,
  /// Solutions found in the repo. Always empty when `visual_studio` is None,
  /// since there would be nothing to open them with.
  pub solutions: Vec<SolutionFile>,
}

/// Build a Command that runs `program` with `args`, going through the shell on
/// Windows because most editor launchers there are `.cmd` shims.
fn launcher(program: &str, args: &[&str]) -> Command {
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("cmd");
    c.arg("/C").arg(program).args(args);
    c.creation_flags(CREATE_NO_WINDOW);
    c
  }
  #[cfg(not(windows))]
  {
    let mut c = Command::new(program);
    c.args(args);
    c
  }
}

/// Whether a launcher command can be resolved. Uses the platform's own lookup
/// (`where` / `which`) rather than walking PATH by hand, so it honours
/// PATHEXT on Windows and shell aliases elsewhere.
fn command_exists(program: &str) -> bool {
  #[cfg(windows)]
  let mut probe = {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("where");
    c.arg(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
  };
  #[cfg(not(windows))]
  let mut probe = {
    let mut c = Command::new("which");
    c.arg(program);
    c
  };

  probe
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// The launcher for `kind`, if that editor is installed.
fn find_launcher(kind: EditorKind) -> Option<&'static str> {
  kind.commands().iter().copied().find(|c| command_exists(c))
}

/// How long a detection result stays good for.
///
/// Detection is expensive: every candidate launcher costs a `where`/`which`
/// process, and a *miss* is the slow case because the whole PATH gets scanned.
/// With 12 candidates plus vswhere that is ~13 process spawns, which is far too
/// much to repeat on each call -- the toolbar asks on every repo open.
///
/// Deliberately a TTL rather than a permanent cache. Someone who installs an
/// editor while GitWyrm is running should see it appear without restarting, so
/// the result has to expire on its own.
const DETECT_TTL: Duration = Duration::from_secs(60);

/// Memoized editor detection, cleared by its own age.
static EDITOR_CACHE: Mutex<Option<(Instant, Vec<InstalledEditor>)>> = Mutex::new(None);

/// Memoized Visual Studio detection. Separate from the editor cache so a
/// machine with no Visual Studio does not re-run vswhere on every call.
static VS_CACHE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);

/// Held for the duration of a probe so concurrent callers run one, not several.
///
/// Separate from the cache mutexes on purpose. The cache lock is only ever
/// taken to read or write the stored value and is never held across the probe:
/// holding it there would make every other caller -- including ones that would
/// have hit a warm cache -- block on process spawns.
static EDITOR_FLIGHT: Mutex<()> = Mutex::new(());
static VS_FLIGHT: Mutex<()> = Mutex::new(());

/// Read the cache if the stored value is still young enough.
///
/// Takes the lock, copies, releases. Never call the probe while holding it.
fn cached<T: Clone>(cache: &Mutex<Option<(Instant, T)>>) -> Option<T> {
  cache
    .lock()
    .unwrap()
    .as_ref()
    .filter(|(at, _)| at.elapsed() < DETECT_TTL)
    .map(|(_, value)| value.clone())
}

/// Run `probe` once even when several callers arrive together.
///
/// The first caller through takes the flight lock and probes. The others block
/// on that lock rather than starting their own probe, then re-check the cache
/// the winner just filled and return it without probing at all. Startup opens
/// several repos at once and every one asks for this, so without the gate the
/// cache only started paying off from the *fourth* caller onward -- the first
/// three each ran a full ~13-process detection concurrently.
fn detect_once<T: Clone>(
  cache: &Mutex<Option<(Instant, T)>>,
  flight: &Mutex<()>,
  probe: impl FnOnce() -> T,
) -> T {
  if let Some(hit) = cached(cache) {
    return hit;
  }

  let _flight = flight.lock().unwrap_or_else(|e| e.into_inner());

  // The winner of the race filled the cache while this caller waited.
  if let Some(hit) = cached(cache) {
    return hit;
  }

  let found = probe();
  *cache.lock().unwrap() = Some((Instant::now(), found.clone()));
  found
}

/// Every editor found on this machine, in `EditorKind::ALL` order.
///
/// Cached for `DETECT_TTL`; see the constant for why this is not computed on
/// every call.
pub fn detect_editors() -> Vec<InstalledEditor> {
  detect_once(&EDITOR_CACHE, &EDITOR_FLIGHT, || {
    EditorKind::ALL
      .iter()
      .filter_map(|&kind| {
        find_launcher(kind).map(|command| InstalledEditor {
          kind,
          label: kind.label().to_string(),
          command: command.to_string(),
        })
      })
      .collect()
  })
}

/// Path to `vswhere.exe`, the tool Visual Studio's installer leaves behind for
/// locating installs. Present on Windows whenever any VS 2017+ has been
/// installed.
#[cfg(windows)]
fn vswhere_path() -> Option<PathBuf> {
  let base = std::env::var("ProgramFiles(x86)")
    .or_else(|_| std::env::var("ProgramFiles"))
    .ok()?;
  let path = PathBuf::from(base)
    .join("Microsoft Visual Studio")
    .join("Installer")
    .join("vswhere.exe");
  path.exists().then_some(path)
}

/// Arguments shared by both vswhere queries.
///
/// `-prerelease` matters: without it vswhere hides Insiders/preview channels,
/// so a machine whose only Visual Studio is, say, "Visual Studio Community
/// 2026" (Insiders) looks like it has none.
///
/// The product ids are listed explicitly rather than passing `-products *`.
/// The VS installer also manages products that are not the IDE -- SQL Server
/// Management Studio 21+ ships as a VS shell app and has its own
/// `productPath` (Ssms.exe) -- so `*` lets `-latest` return something that
/// cannot open a solution. Naming the IDE editions keeps Build Tools and the
/// shell apps out on identity rather than on a heuristic.
#[cfg(windows)]
const VSWHERE_SELECT: [&str; 6] = [
  "-latest",
  "-prerelease",
  "-products",
  "Microsoft.VisualStudio.Product.Enterprise",
  "Microsoft.VisualStudio.Product.Professional",
  "Microsoft.VisualStudio.Product.Community",
];

/// Display name of the newest installed Visual Studio, e.g.
/// "Visual Studio Community 2026". None when VS is not installed.
///
/// Cached for `DETECT_TTL`: the probe spawns vswhere, and the toolbar asks for
/// this on every repo open.
pub fn detect_visual_studio() -> Option<String> {
  detect_once(&VS_CACHE, &VS_FLIGHT, detect_visual_studio_uncached)
}

#[cfg(windows)]
fn detect_visual_studio_uncached() -> Option<String> {
  let vswhere = vswhere_path()?;

  use std::os::windows::process::CommandExt;
  let output = Command::new(vswhere)
    .args(VSWHERE_SELECT)
    .args(["-format", "json", "-utf8"])
    .creation_flags(CREATE_NO_WINDOW)
    .output()
    .ok()?;

  let installs: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
  let install = installs.first()?;
  // Without a productPath there is no devenv.exe to launch.
  install.get("productPath")?.as_str()?;
  install
    .get("displayName")?
    .as_str()
    .map(|s| s.to_string())
}

/// Visual Studio is Windows-only, so there is nothing to look for elsewhere.
#[cfg(not(windows))]
fn detect_visual_studio_uncached() -> Option<String> {
  None
}

/// Absolute path to devenv.exe for the newest install.
///
/// Solutions are launched by shell-open rather than by invoking devenv
/// directly, so nothing in the app calls this. It stays because the detection
/// test asserts that a detected Visual Studio resolves to a real executable --
/// the check that `detect_visual_studio` agrees with vswhere.
#[cfg(all(windows, test))]
fn visual_studio_exe() -> Option<PathBuf> {
  let vswhere = vswhere_path()?;

  use std::os::windows::process::CommandExt;
  let output = Command::new(vswhere)
    .args(VSWHERE_SELECT)
    .args(["-property", "productPath", "-format", "value", "-utf8"])
    .creation_flags(CREATE_NO_WINDOW)
    .output()
    .ok()?;

  let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
  (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Folder names never worth descending into when looking for solutions: build
/// output, dependency caches, and any dotfolder (`.git`, `.vs`, `.idea`).
fn is_skipped_dir(name: &str) -> bool {
  name.starts_with('.')
    || matches!(
      name.to_ascii_lowercase().as_str(),
      "node_modules" | "bin" | "obj" | "target" | "dist" | "build" | "packages" | "vendor"
    )
}

/// Depth below the repo root that the solution scan reaches. The root itself is
/// depth 0, so this covers `MyApp.sln`, `src/MyApp.sln`, and
/// `src/MyApp/MyApp.sln` -- the layouts C# solutions actually use -- without
/// walking a whole tree.
const MAX_SOLUTION_DEPTH: usize = 2;

/// Find `.sln` files at or below `root`, to `MAX_SOLUTION_DEPTH`. Results are
/// sorted by path so the dropdown order is stable between openings.
pub fn find_solutions(root: &Path) -> Vec<SolutionFile> {
  let mut found = Vec::new();
  scan_solutions(root, root, 0, &mut found);
  found.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
  found
}

fn scan_solutions(root: &Path, dir: &Path, depth: usize, found: &mut Vec<SolutionFile>) {
  let Ok(entries) = std::fs::read_dir(dir) else {
    return;
  };

  for entry in entries.flatten() {
    let path = entry.path();
    let Ok(file_type) = entry.file_type() else {
      continue;
    };
    let name = entry.file_name().to_string_lossy().into_owned();

    if file_type.is_dir() {
      if depth < MAX_SOLUTION_DEPTH && !is_skipped_dir(&name) {
        scan_solutions(root, &path, depth + 1, found);
      }
      continue;
    }

    if !path
      .extension()
      .is_some_and(|e| e.eq_ignore_ascii_case("sln"))
    {
      continue;
    }

    let relative = path
      .strip_prefix(root)
      .unwrap_or(&path)
      .to_string_lossy()
      .replace('\\', "/");
    found.push(SolutionFile {
      name,
      relative_path: relative,
      absolute_path: path.to_string_lossy().into_owned(),
    });
  }
}

/// Launch `path` (a folder or a file) in the given editor.
pub fn open_path_in(kind: EditorKind, path: &str) -> Result<(), AppError> {
  let Some(command) = find_launcher(kind) else {
    return Err(AppError::Other(format!(
      "Could not find {}. Install it, or pick a different editor in Settings.",
      kind.label()
    )));
  };

  launcher(command, &[path]).spawn().map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
      AppError::Other(format!(
        "Could not find {}. Install it, or pick a different editor in Settings.",
        kind.label()
      ))
    } else {
      AppError::Io(e)
    }
  })?;
  Ok(())
}

/// Open a `.sln` with whatever the user has set to handle solution files.
///
/// This deliberately does not launch a devenv.exe resolved by vswhere. Windows
/// associates `.sln` with VSLauncher.exe, which reads the version stamped in
/// the solution header and starts the matching Visual Studio -- so a repo
/// pinned to an older VS opens in that VS, not in the newest one installed.
/// Handing the path to the shell keeps that behaviour, and honours the user's
/// choice if they have pointed `.sln` somewhere else entirely.
///
/// `start` needs its first quoted argument treated as the window title, hence
/// the empty `""` before the path; without it a quoted path becomes the title
/// and nothing opens.
#[cfg(windows)]
pub fn open_solution(solution_path: &str) -> Result<(), AppError> {
  use std::os::windows::process::CommandExt;

  Command::new("cmd")
    .args(["/C", "start", "", solution_path])
    .creation_flags(CREATE_NO_WINDOW)
    .spawn()
    .map_err(AppError::Io)?;
  Ok(())
}

#[cfg(not(windows))]
pub fn open_solution(_solution_path: &str) -> Result<(), AppError> {
  Err(AppError::Other(
    "Visual Studio is only available on Windows.".to_string(),
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn build_output_and_dotfolders_are_skipped() {
    assert!(is_skipped_dir("node_modules"));
    assert!(is_skipped_dir("bin"));
    assert!(is_skipped_dir("obj"));
    assert!(is_skipped_dir("Obj"));
    assert!(is_skipped_dir(".git"));
    assert!(is_skipped_dir(".vs"));
    assert!(!is_skipped_dir("src"));
    assert!(!is_skipped_dir("MyApp"));
  }

  #[test]
  fn editor_ids_round_trip_through_json() {
    // These ids are persisted in settings.json, so a rename would silently
    // reset every user's chosen editor.
    let json = serde_json::to_string(&EditorKind::VsCode).unwrap();
    assert_eq!(json, "\"vs_code\"");
    assert_eq!(
      serde_json::from_str::<EditorKind>("\"jetbrains\"").unwrap(),
      EditorKind::Jetbrains
    );
  }

  /// Callers arriving together must produce exactly one probe.
  ///
  /// Startup opens several repos at once and each asks for editor availability,
  /// so before the flight gate the first few callers each ran a full detection
  /// concurrently and only later ones hit the cache. Uses its own cache/flight
  /// pair and a counting probe so the assertion is about the gate itself, not
  /// about whatever is installed on the machine running the test.
  #[test]
  fn concurrent_callers_probe_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    static CACHE: Mutex<Option<(Instant, usize)>> = Mutex::new(None);
    static FLIGHT: Mutex<()> = Mutex::new(());
    let probes = Arc::new(AtomicUsize::new(0));

    let threads: Vec<_> = (0..8)
      .map(|_| {
        let probes = Arc::clone(&probes);
        std::thread::spawn(move || {
          detect_once(&CACHE, &FLIGHT, || {
            probes.fetch_add(1, Ordering::SeqCst);
            // Long enough that the other threads are certainly waiting, so a
            // missing gate would show up as extra probes rather than by luck.
            std::thread::sleep(Duration::from_millis(50));
            42
          })
        })
      })
      .collect();

    let results: Vec<usize> = threads.into_iter().map(|t| t.join().unwrap()).collect();

    assert_eq!(probes.load(Ordering::SeqCst), 1, "8 concurrent callers must probe once");
    assert!(results.iter().all(|&v| v == 42), "every caller gets the value");
  }

  /// The cache lock must never be held across a probe, or a caller that would
  /// have hit a warm cache ends up blocked behind process spawns -- which is
  /// the UI hang the cache exists to prevent.
  #[test]
  fn a_warm_cache_hit_does_not_wait_on_an_in_flight_probe() {
    static CACHE: Mutex<Option<(Instant, usize)>> = Mutex::new(None);
    static FLIGHT: Mutex<()> = Mutex::new(());

    // Prime it, so later callers should be served from the stored value.
    detect_once(&CACHE, &FLIGHT, || 7);

    // Start a slow probe against a *different* pair to hold a flight lock, then
    // confirm the primed cache still answers immediately.
    static OTHER_CACHE: Mutex<Option<(Instant, usize)>> = Mutex::new(None);
    static OTHER_FLIGHT: Mutex<()> = Mutex::new(());
    let slow = std::thread::spawn(|| {
      detect_once(&OTHER_CACHE, &OTHER_FLIGHT, || {
        std::thread::sleep(Duration::from_millis(200));
        1
      })
    });

    let started = Instant::now();
    assert_eq!(detect_once(&CACHE, &FLIGHT, || panic!("must not re-probe")), 7);
    let waited = started.elapsed();

    slow.join().unwrap();
    assert!(
      waited < Duration::from_millis(50),
      "a warm hit must not block on unrelated detection, waited {waited:?}"
    );
  }

  /// The second call must not re-probe. Detection costs a process per
  /// candidate launcher, and a *miss* is the expensive case because the whole
  /// PATH gets scanned -- so an uncached call on a machine with no editors
  /// installed is the worst case, not the cheapest. This ran on every repo
  /// open and was a large part of a multi-second startup stall.
  #[test]
  fn editor_detection_is_cached() {
    let first = Instant::now();
    let a = detect_editors();
    let cold = first.elapsed();

    let second = Instant::now();
    let b = detect_editors();
    let warm = second.elapsed();

    // Same answer both times.
    assert_eq!(a.len(), b.len());

    // A cache hit clones a small Vec; the cold path spawns processes. Asserting
    // against the cold time rather than a fixed number keeps this meaningful on
    // a fast machine and honest on a slow one.
    assert!(
      warm <= cold,
      "second detect_editors() should not re-probe: cold {cold:?}, warm {warm:?}"
    );
    assert!(
      warm < Duration::from_millis(50),
      "a cache hit should be effectively free, took {warm:?}"
    );
  }

  /// Detection has to survive the real vswhere on a real machine, where the
  /// only install may be a prerelease channel (Insiders) and may be a version
  /// that no longer carries the CoreIde component id. Both of those silently
  /// returned "not installed" before. Skipped when vswhere is absent, so this
  /// stays green on machines and CI runners without Visual Studio.
  #[cfg(windows)]
  #[test]
  fn visual_studio_detection_agrees_with_vswhere() {
    let Some(vswhere) = vswhere_path() else {
      return;
    };

    use std::os::windows::process::CommandExt;
    let output = Command::new(vswhere)
      .args(VSWHERE_SELECT)
      .args(["-property", "productPath", "-format", "value", "-utf8"])
      .creation_flags(CREATE_NO_WINDOW)
      .output()
      .expect("vswhere should run");
    let has_devenv = !String::from_utf8_lossy(&output.stdout).trim().is_empty();

    assert_eq!(
      detect_visual_studio().is_some(),
      has_devenv,
      "detection disagreed with vswhere about whether Visual Studio is installed"
    );
    if has_devenv {
      let exe = visual_studio_exe().expect("a detected Visual Studio must resolve to an exe");
      assert!(exe.exists(), "resolved Visual Studio exe does not exist: {exe:?}");
      // The product filter exists to keep non-IDE products that the VS
      // installer also manages -- SSMS 21+ is the one that bit us -- from being
      // detected as Visual Studio. Those resolve to their own exe (Ssms.exe),
      // never devenv.exe, so this is the assertion that would have caught it.
      let name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
      assert_eq!(
        name, "devenv.exe",
        "detected Visual Studio should be the IDE, got {exe:?}"
      );
    }
  }

  /// A real C# repo keeps solutions in nested folders, so the scan has to reach
  /// them without a full-tree walk. Skipped when the repo is not checked out.
  #[test]
  fn solution_scan_finds_solutions_in_a_real_csharp_repo() {
    let repo = Path::new("C:/code/nuget-compass");
    if !repo.exists() {
      return;
    }

    let found = find_solutions(repo);
    assert!(
      found
        .iter()
        .any(|s| s.relative_path == "fixtures/net8-mixed-versions/net8-mixed-versions.sln"),
      "expected the nested fixture solution, got {:?}",
      found.iter().map(|s| &s.relative_path).collect::<Vec<_>>()
    );
  }

  #[test]
  fn solution_scan_finds_nested_solutions_and_stops_at_depth() {
    let temp = std::env::temp_dir().join(format!("gitwyrm-sln-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);

    // Root, one level down, two levels down, and one level too deep.
    std::fs::create_dir_all(temp.join("src/MyApp/Deep")).unwrap();
    std::fs::create_dir_all(temp.join("node_modules/pkg")).unwrap();
    std::fs::write(temp.join("Root.sln"), "").unwrap();
    std::fs::write(temp.join("src/Mid.sln"), "").unwrap();
    std::fs::write(temp.join("src/MyApp/Deep.sln"), "").unwrap();
    std::fs::write(temp.join("src/MyApp/Deep/TooDeep.sln"), "").unwrap();
    std::fs::write(temp.join("node_modules/pkg/Ignored.sln"), "").unwrap();

    let found = find_solutions(&temp);
    let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"Root.sln"));
    assert!(names.contains(&"Mid.sln"));
    assert!(names.contains(&"Deep.sln"));
    assert!(!names.contains(&"TooDeep.sln"), "scan should stop at depth 2");
    assert!(!names.contains(&"Ignored.sln"), "node_modules should be skipped");
    // Relative paths use forward slashes regardless of platform.
    let mid = found.iter().find(|s| s.name == "Mid.sln").unwrap();
    assert_eq!(mid.relative_path, "src/Mid.sln");

    let _ = std::fs::remove_dir_all(&temp);
  }
}
