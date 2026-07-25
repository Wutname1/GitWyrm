//! Explorer's "Open with GitWyrm" right-click entry.
//!
//! This is a pair of plain shell verbs under `HKEY_CURRENT_USER`, the same
//! shape Visual Studio's `AnyCode` verb and GitKraken use. Windows 11 promotes
//! verbs like these into the top-level context menu -- no packaged app and no
//! `IExplorerCommand` handler is involved. Writing under HKCU also means no
//! elevation prompt, which matches the installer's `currentUser` install mode.
//!
//! Two keys are written, because Explorer treats them as different targets:
//!
//! - `Directory\shell\GitWyrm` fires when you right-click a folder. Explorer
//!   substitutes the clicked folder for `%1`.
//! - `Directory\Background\shell\GitWyrm` fires when you right-click empty
//!   space inside an open folder. That one needs `%V`; `%1` is empty there.
//!
//! The registry is the single source of truth for whether the entry exists --
//! deliberately not mirrored into settings.json. The NSIS installer writes
//! these keys and the uninstaller removes them, so a cached flag in our own
//! settings would drift out of sync with reality the first time someone
//! reinstalls or removes the app.

use crate::error::AppError;

/// Verb key name. Shared by both targets and by the NSIS installer scripts --
/// keep them in step if this ever changes.
#[cfg(windows)]
const VERB: &str = "GitWyrm";

/// Menu label. Plain language, no jargon: this is aimed at someone who does not
/// necessarily know what a "repository" is.
#[cfg(windows)]
const LABEL: &str = "Open with GitWyrm";

/// The two parent paths under `HKCU\Software\Classes`, each with the argument
/// placeholder Explorer expands for that target.
#[cfg(windows)]
const TARGETS: [(&str, &str); 2] = [
  (r"Software\Classes\Directory\shell", "%1"),
  (r"Software\Classes\Directory\Background\shell", "%V"),
];

/// Absolute path to the running executable, quoted for a registry command
/// string.
#[cfg(windows)]
fn exe_path() -> Result<String, AppError> {
  let exe = std::env::current_exe()?;
  Ok(exe.to_string_lossy().into_owned())
}

/// Whether the right-click entry is currently registered.
///
/// Reports `true` only when every target is present, so a half-written state
/// (an interrupted install, a hand-edited registry) shows as off and the
/// toggle repairs it rather than reporting success it cannot back up.
#[tauri::command]
#[specta::specta]
pub fn context_menu_registered() -> Result<bool, AppError> {
  #[cfg(windows)]
  {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (parent, _) in TARGETS {
      let path = format!(r"{parent}\{VERB}\command");
      let Ok(key) = hkcu.open_subkey_with_flags(&path, KEY_READ) else {
        return Ok(false);
      };
      // A key with an empty command launches nothing. Treat that as absent.
      let command: String = key.get_value("").unwrap_or_default();
      if command.trim().is_empty() {
        return Ok(false);
      }
    }
    Ok(true)
  }
  #[cfg(not(windows))]
  Ok(false)
}

/// Add or remove Explorer's "Open with GitWyrm" entry.
///
/// Registering rewrites the keys unconditionally so the command always points
/// at the current executable -- an app that moved (or updated into a new
/// versioned folder) repairs itself when the user toggles this off and on.
#[tauri::command]
#[specta::specta]
pub fn set_context_menu_registered(enabled: bool) -> Result<(), AppError> {
  #[cfg(windows)]
  {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    if !enabled {
      for (parent, _) in TARGETS {
        // Removing an entry that was never there is a success, not an error.
        match hkcu.delete_subkey_all(format!(r"{parent}\{VERB}")) {
          Ok(()) => {}
          Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
          Err(e) => return Err(e.into()),
        }
      }
      log::info!("Removed the Explorer right-click entry");
      return Ok(());
    }

    let exe = exe_path()?;
    for (parent, arg) in TARGETS {
      let (key, _) = hkcu.create_subkey(format!(r"{parent}\{VERB}"))?;
      key.set_value("", &LABEL)?;
      // Windows 11 appears to need an icon before it will lift a verb into the
      // top-level menu -- both entries confirmed to land there set one.
      key.set_value("Icon", &exe)?;

      let (command, _) = hkcu.create_subkey(format!(r"{parent}\{VERB}\command"))?;
      command.set_value("", &format!("\"{exe}\" \"{arg}\""))?;
    }
    log::info!("Registered the Explorer right-click entry for {exe}");
    Ok(())
  }
  #[cfg(not(windows))]
  {
    let _ = enabled;
    Err(AppError::Other(
      "The right-click entry is only available on Windows".into(),
    ))
  }
}

#[cfg(all(test, windows))]
mod tests {
  use super::*;

  /// Both targets must be registered under HKCU (no elevation) and must use the
  /// placeholder Explorer actually expands for that target. `%1` is empty when
  /// right-clicking folder background, so using it there would silently launch
  /// the app with no folder.
  #[test]
  fn targets_use_the_right_placeholder() {
    assert_eq!(TARGETS.len(), 2);
    for (parent, arg) in TARGETS {
      assert!(parent.starts_with(r"Software\Classes\"), "{parent} must be HKCU-relative");
      if parent.contains("Background") {
        assert_eq!(arg, "%V", "folder background expands %V, not %1");
      } else {
        assert_eq!(arg, "%1");
      }
    }
  }

  /// The command string has to survive a path with spaces ("C:\Program Files\"),
  /// which means both the exe and the placeholder need their own quotes.
  #[test]
  fn command_string_quotes_both_parts() {
    let exe = r"C:\Program Files\GitWyrm\GitWyrm.exe";
    let command = format!("\"{exe}\" \"{}\"", "%1");
    assert_eq!(
      command,
      r#""C:\Program Files\GitWyrm\GitWyrm.exe" "%1""#
    );
  }
}
