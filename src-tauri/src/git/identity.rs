//! The name and email git stamps on every commit.
//!
//! Read from and written to the user's global git config, which is where git
//! itself looks when a repository does not override it. Kept separate from
//! GitWyrm's own settings file on purpose: this is git's setting, shared with
//! every other tool on the machine, and duplicating it into our settings would
//! create a second source of truth that silently drifts.

use crate::error::AppError;

/// Who git thinks the user is. Either field can be empty when git has never
/// been set up, which is the normal state on a fresh machine.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

impl GitIdentity {
    /// True when both halves are present, which is what git requires before it
    /// will let a commit through.
    pub fn is_complete(&self) -> bool {
        !self.name.trim().is_empty() && !self.email.trim().is_empty()
    }
}

/// Read the effective identity: repository override if there is one, otherwise
/// the global config. Missing values come back empty rather than as an error,
/// because "not set up yet" is an expected state the UI has to render.
pub fn read_identity() -> GitIdentity {
    // open_default walks the system, global, and (when inside one) repository
    // configs in git's own precedence order.
    let Ok(config) = git2::Config::open_default() else {
        return GitIdentity::default();
    };

    GitIdentity {
        name: config.get_string("user.name").unwrap_or_default(),
        email: config.get_string("user.email").unwrap_or_default(),
    }
}

/// Write the identity to the user's global git config.
///
/// Global rather than per-repository: someone setting their name in onboarding
/// means "this is who I am", not "this is who I am in this one folder".
pub fn write_identity(name: &str, email: &str) -> Result<(), AppError> {
    let name = name.trim();
    let email = email.trim();

    if name.is_empty() {
        return Err(AppError::Other(
            "Enter the name to put on your commits.".into(),
        ));
    }
    if email.is_empty() {
        return Err(AppError::Other(
            "Enter the email to put on your commits.".into(),
        ));
    }
    // Not full RFC validation - just enough to catch an obvious slip before it
    // ends up baked into commit history where it is painful to change.
    if !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        return Err(AppError::Other(
            "That email does not look right. It should look like you@example.com.".into(),
        ));
    }

    // open_default is read-oriented and can resolve to a repository config when
    // the process is inside one. Opening the global file directly guarantees the
    // write lands where every repository will see it.
    let path = git2::Config::find_global()
        .or_else(|_| default_global_config_path())
        .map_err(|e| AppError::Other(format!("Could not find your git settings file: {e}")))?;

    let mut config = git2::Config::open(&path)?;
    config.set_str("user.name", name)?;
    config.set_str("user.email", email)?;
    Ok(())
}

/// Path to the user's global config, creating nothing but naming where it goes.
///
/// Shared with the profiles module, which writes the same file when the active
/// profile changes.
pub fn global_config_path() -> Result<std::path::PathBuf, AppError> {
    git2::Config::find_global()
        .or_else(|_| default_global_config_path())
        .map_err(|e| AppError::Other(format!("Could not find your git settings file: {e}")))
}

/// Where the global config lives when git has never written one.
///
/// `find_global` fails if `~/.gitconfig` does not exist yet, which is exactly
/// the fresh-machine case this feature is for. Returning the conventional path
/// lets `Config::open` create it.
fn default_global_config_path() -> Result<std::path::PathBuf, git2::Error> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| git2::Error::from_str("no home directory"))?;
    Ok(std::path::PathBuf::from(home).join(".gitconfig"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_needs_both_halves() {
        let both = GitIdentity {
            name: "Ada".into(),
            email: "ada@example.com".into(),
        };
        assert!(both.is_complete());

        for partial in [
            GitIdentity {
                name: "Ada".into(),
                email: String::new(),
            },
            GitIdentity {
                name: String::new(),
                email: "ada@example.com".into(),
            },
            GitIdentity::default(),
        ] {
            assert!(!partial.is_complete());
        }
    }

    #[test]
    fn whitespace_only_values_do_not_count_as_set() {
        // Git accepts a whitespace name, producing commits authored by "   ".
        let blank = GitIdentity {
            name: "   ".into(),
            email: "\t".into(),
        };
        assert!(!blank.is_complete());
    }

    #[test]
    fn reading_never_fails_even_with_no_git_config() {
        // The fresh-machine case: the UI renders "not set up yet" from this, so it
        // must return empties rather than an error.
        let identity = read_identity();
        // Nothing to assert about the values (they depend on the machine); the
        // point is that the call returns at all.
        let _ = identity.is_complete();
    }

    #[test]
    fn writing_rejects_a_blank_name_or_email() {
        assert!(write_identity("", "a@b.com").is_err());
        assert!(write_identity("Ada", "  ").is_err());
    }

    #[test]
    fn writing_rejects_an_email_without_a_plausible_shape() {
        // Caught before it is baked into history, where changing it means a rewrite.
        for bad in ["not-an-email", "@example.com", "ada@"] {
            assert!(
                write_identity("Ada", bad).is_err(),
                "expected {bad} to be rejected"
            );
        }
    }
}
