//! Exercises the bundled git and gpg against the real unpacked binaries.
//!
//! These are ignored by default because they need `src-tauri/resources` to be
//! populated, which only happens after `.github/scripts/fetch-bundled-tools.sh`
//! has run. CI runs them right after the fetch step; locally:
//!
//!   cargo test --test bundled_tools -- --ignored
//!
//! What they protect: the pinned MSYS DLL list in the fetch script, and the
//! cygdrive path translation gpg needs when it is relocated. Both fail in ways
//! that only show up when the binaries actually run, not at compile time.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The unpacked bundle, or None when the tools have not been fetched.
fn bundle() -> Option<PathBuf> {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
  root.join("git/cmd/git.exe").is_file().then_some(root)
}

fn run(program: &Path, args: &[&str]) -> (bool, String) {
  let out = Command::new(program)
    .args(args)
    .output()
    .unwrap_or_else(|e| panic!("failed to run {}: {e}", program.display()));
  let text = format!(
    "{}{}",
    String::from_utf8_lossy(&out.stdout),
    String::from_utf8_lossy(&out.stderr)
  );
  (out.status.success(), text)
}

#[test]
#[ignore]
fn bundled_git_runs() {
  let Some(root) = bundle() else {
    eprintln!("resources/ not populated; run fetch-bundled-tools.sh first");
    return;
  };
  let (ok, out) = run(&root.join("git/cmd/git.exe"), &["--version"]);
  assert!(ok, "bundled git failed: {out}");
  assert!(out.contains("git version"), "unexpected output: {out}");
}

#[test]
#[ignore]
fn bundled_gpg_runs_with_the_pinned_dll_set() {
  // The fetch script copies only the MSYS libraries GnuPG links against rather
  // than all 73 in usr/bin. A missing one shows up here and nowhere earlier.
  let Some(root) = bundle() else {
    eprintln!("resources/ not populated; run fetch-bundled-tools.sh first");
    return;
  };
  let (ok, out) = run(&root.join("gpg/gpg.exe"), &["--version"]);
  assert!(ok, "bundled gpg failed to start: {out}");
  assert!(out.contains("gpg (GnuPG)"), "unexpected output: {out}");
}

#[test]
#[ignore]
fn bundled_gpg_ships_its_agent_and_pinentry() {
  // gpg without its agent cannot generate a key or sign anything, and without
  // pinentry it cannot ask for a passphrase. Both are easy to leave behind.
  let Some(root) = bundle() else {
    eprintln!("resources/ not populated; run fetch-bundled-tools.sh first");
    return;
  };
  for required in ["gpg/gpg-agent.exe", "gpg/gpgconf.exe", "gpg/pinentry-w32.exe"] {
    assert!(
      root.join(required).is_file(),
      "{required} missing from the bundle"
    );
  }
}

#[test]
#[ignore]
fn bundled_git_carries_credential_manager() {
  // MinGit includes Git Credential Manager, which is what lets HTTPS push and
  // pull work on a machine with no git installed. Losing it would break auth
  // for exactly the users the bundle exists to serve.
  let Some(root) = bundle() else {
    eprintln!("resources/ not populated; run fetch-bundled-tools.sh first");
    return;
  };
  assert!(
    root.join("git/mingw64/bin/git-credential-manager.exe").is_file(),
    "git-credential-manager.exe missing; bundled HTTPS auth would break"
  );
}

#[test]
#[ignore]
fn a_relocated_gpg_can_generate_a_key_and_sign() {
  // The end-to-end proof that the bundle is usable: gpg needs an explicit
  // agent-program (its compiled-in /usr/bin/gpg-agent does not exist once
  // relocated) and a /cygdrive-style GNUPGHOME. Both are what the app does in
  // git::signing; this checks the combination actually works.
  let Some(root) = bundle() else {
    eprintln!("resources/ not populated; run fetch-bundled-tools.sh first");
    return;
  };

  // Short path: gpg-agent refuses to start when its socket path grows too long.
  let home = PathBuf::from(r"C:\gwtest-gnupg");
  let _ = std::fs::remove_dir_all(&home);
  std::fs::create_dir_all(&home).unwrap();

  let gpg = root.join("gpg/gpg.exe");
  let agent = root
    .join("gpg/gpg-agent.exe")
    .to_string_lossy()
    .replace('\\', "/");
  std::fs::write(home.join("gpg.conf"), format!("agent-program {agent}\n")).unwrap();

  let cyg = "/cygdrive/c/gwtest-gnupg";
  let out = Command::new(&gpg)
    .env("GNUPGHOME", cyg)
    .args([
      "--batch", "--passphrase", "", "--quick-generate-key",
      "Bundle Test <bundle@gitwyrm.invalid>", "ed25519", "sign", "never",
    ])
    .output()
    .expect("run gpg");
  let text = String::from_utf8_lossy(&out.stderr).into_owned();

  // Stop the agent before asserting, so a failure does not leave it holding
  // the directory open for the next run.
  let _ = Command::new(root.join("gpg/gpgconf.exe"))
    .env("GNUPGHOME", cyg)
    .arg("--kill")
    .arg("all")
    .output();

  assert!(out.status.success(), "key generation failed: {text}");
  let _ = std::fs::remove_dir_all(&home);
}
