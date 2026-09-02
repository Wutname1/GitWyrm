//! Undoes the environment an AppImage imposes on child processes.
//!
//! The AppImage runtime prepends its own bundled libraries to `LD_LIBRARY_PATH`
//! (and points several loader/module variables at its private tree) so the
//! packaged app finds the exact library versions it was built against. Those
//! variables are exported into the process environment, which means every
//! process GitWyrm spawns inherits them too.
//!
//! That is fine for our own binary and catastrophic for everyone else's. The
//! tools we shell out to - git, gpg, ssh-keygen, gh, the user's editor - are
//! *system* binaries linked against *system* libraries. Handing them our
//! bundled copies mixes two independent library sets, and the loader fails at
//! whichever symbol the older copy happens to lack.
//!
//! The real-world failure this was written for: `git-remote-https` links
//! `libcurl-gnutls.so.4`, which needs a symbol added in nghttp2 1.50. Under the
//! AppImage it resolved our older bundled `libnghttp2` instead of the system
//! one, so every clone, fetch and push died with a symbol lookup error before a
//! single byte reached the network. The same git worked fine from a terminal,
//! which is what makes this shape of bug so confusing to report.
//!
//! The AppImage runtime saves each variable's pre-launch value as `<VAR>_ORIG`
//! before overwriting it, so the fix is to put the original back: restore from
//! `<VAR>_ORIG` when it exists, and otherwise remove the variable outright.
//!
//! Applied unconditionally on Linux rather than gated behind an AppImage check.
//! Off an AppImage none of these `_ORIG` variables exist and none of the
//! originals were rewritten, so the restore is a no-op - and a no-op costs less
//! than a detection heuristic that can be wrong.

/// Variables the AppImage runtime rewrites to point into its bundled tree.
///
/// The loader entries (`LD_*`) are the ones that cause hard symbol failures.
/// The rest steer plugin and data lookup for the GTK/GLib stack and matter for
/// spawned GUI programs - an editor launched from GitWyrm should read the
/// system's schemas and pixbuf loaders, not ours.
#[cfg(target_os = "linux")]
const APPIMAGE_OVERRIDDEN_VARS: &[&str] = &[
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GIO_MODULE_DIR",
    "GSETTINGS_SCHEMA_DIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GST_PLUGIN_SYSTEM_PATH",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "QT_PLUGIN_PATH",
    "PYTHONPATH",
    "PERLLIB",
    "XDG_DATA_DIRS",
];

/// Restore the pre-AppImage environment on a command about to be spawned.
///
/// Call this on every `Command` that runs a program we did not ship. Skipping
/// one leaves exactly the bug described above, reachable only from that one
/// call site and only for users on the AppImage build.
///
/// A no-op on every platform but Linux: no other bundle format rewrites the
/// loader environment of its children.
/// The slice of a command builder this module needs.
///
/// Exists because the agent transport builds `tokio::process::Command` while
/// everything else builds `std::process::Command`. The two are unrelated types
/// with identical environment methods, and a spawn site that could not take the
/// scrub would be a hole in exactly the place holes are hard to notice.
pub trait EnvMut {
    fn set_var(&mut self, key: &str, value: std::ffi::OsString);
    fn unset_var(&mut self, key: &str);
}

impl EnvMut for std::process::Command {
    fn set_var(&mut self, key: &str, value: std::ffi::OsString) {
        self.env(key, value);
    }
    fn unset_var(&mut self, key: &str) {
        self.env_remove(key);
    }
}

impl EnvMut for tokio::process::Command {
    fn set_var(&mut self, key: &str, value: std::ffi::OsString) {
        self.env(key, value);
    }
    fn unset_var(&mut self, key: &str) {
        self.env_remove(key);
    }
}

#[cfg(target_os = "linux")]
pub fn scrub_bundled_env<C: EnvMut>(cmd: &mut C) {
    for var in APPIMAGE_OVERRIDDEN_VARS {
        match std::env::var_os(format!("{var}_ORIG")) {
            // The runtime saved what was there before it took over. Put it back
            // verbatim, so the child sees the environment it would have seen had
            // it been launched from a shell.
            Some(original) => {
                cmd.set_var(var, original);
            }
            // No saved value means the variable was not set before launch, so
            // anything present now was added by the runtime. Remove it rather
            // than blanking it: an empty `LD_LIBRARY_PATH` is not the same as an
            // absent one - some loaders read "" as the current directory.
            None => {
                cmd.unset_var(var);
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn scrub_bundled_env<C: EnvMut>(_cmd: &mut C) {}

/// True when the app is running from an AppImage. Only used to note the fact at
/// startup - the scrub itself does not branch on it.
#[cfg(target_os = "linux")]
pub fn is_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some() || std::env::var_os("APPDIR").is_some()
}

#[cfg(not(target_os = "linux"))]
pub fn is_appimage() -> bool {
    false
}

/// Record the launch environment once at startup.
///
/// This exists because the failure it guards against is invisible in a bug
/// report: the user sees a symbol error from a library they never installed,
/// and the same command works in their terminal. Knowing from the log whether
/// they were on the AppImage turns that into a one-line diagnosis.
pub fn log_launch_environment() {
    if is_appimage() {
        log::info!("running from AppImage; scrubbing bundled library paths from child processes");
    }
}
