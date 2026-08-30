//! Which agent command-line tools GitWyrm knows how to drive, as data.
//!
//! Every fact that used to be a hardcoded `"copilot"` string lives here: the
//! names a binary can have on disk, the arguments that put it into Agent
//! Client Protocol stdio mode, how to ask it its version, and -- the part that
//! matters most -- how (or whether) it can be told to refuse a tool.
//!
//! The point of the table is that `acp.rs` already speaks plain ACP. Nothing
//! about the protocol is Copilot-specific; the lock-in was entirely in
//! discovery. So adding a tool is adding a row, not writing a new transport.
//!
//! Every row below was checked against a real install rather than
//! documentation, because these tools' `--help` output and their published
//! docs disagree often enough that only the binary is authoritative.
//!
//! ## What tool denial actually is
//!
//! ACP does NOT standardise tool restriction. `session/new` carries only
//! `cwd`, `additionalDirectories`, `mcpServers` and `_meta`; there is no
//! protocol-level way to say "this session may not write files". So every
//! agent has its own answer, and some have no answer at all -- see
//! [`Denial`]. That is why this is the security-critical half of the table:
//! GitWyrm's read-only promise for Ask, Explain, Review, Summarize and a Plan
//! before Start rests on the launched tool being genuinely unable to write,
//! not on it being asked politely.

use std::path::PathBuf;

/// How a tool can be told, at launch, that it may not use something.
///
/// Modelled as a capability rather than a list of flags because the shapes are
/// genuinely different: one is a repeated command-line flag, one travels
/// inside the protocol, one is a whole-session mode with no per-tool
/// granularity, and one does not exist. Collapsing them into "a list of args"
/// would hide the fact that the last one cannot be trusted with read-only
/// work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// A repeated launch flag naming each denied tool, e.g. `--deny-tool=write`.
    ///
    /// The strongest of the four: it is applied before the agent starts, and
    /// per the Copilot CLI's own `copilot help permissions`, a denial takes
    /// precedence over every allow rule the tool might later apply to itself,
    /// including `--allow-all-tools`. Nothing the model, a config file, or a
    /// remembered approval says afterwards can widen it.
    LaunchFlags {
        /// The flag, written so the tool name is appended after `=`.
        deny_flag: &'static str,
    },
    /// Denial travels in the `session/new` request's `_meta` extension slot
    /// rather than on the command line.
    ///
    /// The Claude adapter takes `_meta.claudeCode.options.disallowedTools` as
    /// a list of tool names. Weaker than a launch flag only in that it is
    /// applied at session open rather than process start; within the session
    /// it is equally binding, and GitWyrm opens exactly one session per run.
    SessionMeta,
    /// No per-tool control, but a whole-session read-only mode.
    ///
    /// Gemini's `--approval-mode=plan` is the closest thing it has: the agent
    /// plans and reads but does not act. It cannot express "you may write but
    /// not run shell commands", so it is usable for read-only work and NOT
    /// usable where a partial denial is needed.
    ReadOnlyMode {
        /// The complete flag, mode value included.
        flag: &'static str,
    },
    /// The tool offers no way at all to restrict what it may do.
    ///
    /// This is not a gap to be worked around with a prompt. A prompt is a
    /// request; the read-only guarantee has to be a bound. An agent with this
    /// capability is refused for every read-only piece of work -- see
    /// [`AgentSpec::can_guarantee_read_only`] and `select::choose`.
    None,
}

impl Denial {
    /// Whether launching under this capability can actually stop the tool from
    /// changing files.
    ///
    /// [`Denial::None`] is the only `false`. Everything else either names the
    /// tool it must not use or puts the whole session in a mode that cannot
    /// act.
    pub fn can_enforce_read_only(self) -> bool {
        !matches!(self, Denial::None)
    }
}

/// What one agent calls the three things GitWyrm gates.
///
/// A struct of three named fields rather than a list, so a row cannot quietly
/// omit one: forgetting the write name is the difference between a read-only
/// session and a session that can edit the user's files.
#[derive(Debug, Clone, Copy)]
pub struct DeniableTools {
    pub shell: &'static [&'static str],
    pub network: &'static [&'static str],
    pub write: &'static [&'static str],
}

impl DeniableTools {
    /// Translates one of GitWyrm's own capability words into this tool's name
    /// for it, or `None` when the tool has no such concept.
    pub(crate) fn name_for(&self, gitwyrm_name: &str) -> &'static [&'static str] {
        match gitwyrm_name {
            "shell" => self.shell,
            "url" => self.network,
            "write" => self.write,
            _ => &[],
        }
    }
}

/// The empty set, for agents whose denial takes no tool names.
const NO_TOOL_NAMES: DeniableTools = DeniableTools {
    shell: &[],
    network: &[],
    write: &[],
};

/// Everything GitWyrm needs to know to find and start one agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentSpec {
    /// Stable machine name, used in settings and in a provider override.
    /// Lower-case, no spaces, never shown to the user as-is.
    pub id: &'static str,
    /// What the user sees. Written the way the tool's own makers write it.
    pub display_name: &'static str,
    /// Binary names to look for, best first. On Windows this must include the
    /// shim extensions -- see [`AgentSpec::candidate_names`].
    pub windows_names: &'static [&'static str],
    /// Binary names on everything that is not Windows.
    pub unix_names: &'static [&'static str],
    /// Arguments that start the tool's ACP server on stdin/stdout.
    pub acp_args: &'static [&'static str],
    /// Arguments that make the tool print its version and exit.
    pub version_args: &'static [&'static str],
    /// How this tool can be told to refuse a tool at launch, if it can.
    pub denial: Denial,
    /// Names this tool uses for the things GitWyrm denies: running shell
    /// commands, reaching the network, and writing files.
    ///
    /// Empty when [`Self::denial`] does not take per-tool names
    /// ([`Denial::ReadOnlyMode`], [`Denial::None`]), because there is nothing
    /// to name.
    pub tool_names: DeniableTools,
    /// Where to send someone who wants this tool.
    ///
    /// One URL serves both states: it is the install page for a tool that is
    /// missing and the documentation for one that is present, so the row needs
    /// a single link control whose wording changes rather than two controls.
    /// Point it at the page that actually explains installing, not a product
    /// home page -- the whole value of this field is that it ends the "not
    /// found on this machine" dead end.
    pub homepage_url: &'static str,
    /// The command that installs this tool, shown to be read and copied and
    /// never run by GitWyrm.
    ///
    /// Never executed: running an install command on someone's behalf means
    /// choosing a package manager for them and writing outside anywhere
    /// GitWyrm owns. Showing it lets them decide.
    pub install_hint: &'static str,
}

impl AgentSpec {
    /// Binary names to try on this platform, best first.
    ///
    /// Windows is the case that matters. `npm install -g @github/copilot` --
    /// the install method GitHub documents first -- writes `copilot.cmd` and
    /// `copilot.ps1` and no `.exe` at all. Looking only for `copilot.exe`
    /// once reported "not installed" on a machine where the CLI was on PATH
    /// and working, which would have been every Windows user who followed the
    /// documented instructions. Every npm-installed tool in this table has the
    /// same shape, so every row lists the shim variants.
    ///
    /// `.cmd` is ordered before the bare name because a bare `copilot` on
    /// Windows is the shell script meant for Git Bash, which `Command::new`
    /// cannot execute directly.
    pub fn candidate_names(&self) -> &'static [&'static str] {
        if cfg!(windows) {
            self.windows_names
        } else {
            self.unix_names
        }
    }

    /// Whether this tool can be trusted with work that must not change
    /// anything.
    ///
    /// The whole safety rule in one place: [`Denial::None`] cannot, everything
    /// else can. `select::choose` is what actually refuses; this is the fact
    /// it refuses on.
    pub fn can_guarantee_read_only(&self) -> bool {
        self.denial.can_enforce_read_only()
    }

    /// The launch arguments for one run: ACP mode, plus whatever this tool's
    /// denial capability needs on the command line.
    ///
    /// `denied` names the tools to refuse, in GitWyrm's own vocabulary
    /// (`shell`, `url`, `write`) -- translated here into whatever this tool
    /// calls them. A name this tool has no word for is skipped rather than
    /// passed through, because an unrecognised `--deny-tool` value is
    /// silently ignored by the CLI rather than rejected, which would look
    /// like it worked while filtering nothing.
    ///
    /// [`Denial::SessionMeta`] contributes nothing here: its denial rides in
    /// `session/new` instead (see [`Self::session_meta`]).
    pub fn launch_args(&self, denied: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = self.acp_args.iter().map(|a| (*a).to_string()).collect();
        match self.denial {
            Denial::LaunchFlags { deny_flag } => {
                for tool in denied {
                    for name in self.tool_names.name_for(tool) {
                        args.push(format!("{deny_flag}={name}"));
                    }
                }
            }
            Denial::ReadOnlyMode { flag } => {
                // No per-tool granularity: the mode is either on or off, and
                // on means the session cannot change anything at all. So it
                // turns on for a WRITE denial specifically, not for any
                // denial.
                //
                // "any denial" was wrong in a way that hid behind a passing
                // test: `shell` and `url` are denied on every run, including
                // Fix, so the flag was added unconditionally and a Fix
                // session launched unable to edit a file -- the one thing it
                // exists to do. The test only checked the empty-denial case,
                // which production never produces.
                //
                // The cost of this narrower rule is that a Fix session on
                // such a tool keeps shell and network access it was meant to
                // lose. That is the honest trade for a tool with one switch,
                // and it is why `Denial::ReadOnlyMode` is not treated as
                // equivalent to per-tool denial anywhere the promise matters.
                if denied.contains(&"write") {
                    args.push(flag.to_string());
                }
            }
            Denial::SessionMeta | Denial::None => {}
        }
        args
    }

    /// The `_meta` object for this tool's `session/new`, if it needs one.
    ///
    /// Only [`Denial::SessionMeta`] returns anything. The shape is the Claude
    /// adapter's own: `_meta.claudeCode.options.disallowedTools`, a list of
    /// tool names.
    pub fn session_meta(&self, denied: &[&str]) -> Option<serde_json::Value> {
        if self.denial != Denial::SessionMeta {
            return None;
        }
        let names: Vec<&str> = denied
            .iter()
            .flat_map(|t| self.tool_names.name_for(t).iter().copied())
            .collect();
        if names.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "claudeCode": { "options": { "disallowedTools": names } }
        }))
    }
}

/// Every agent this build knows how to drive.
///
/// Ordered with the default first. Each row's launch line and denial
/// capability was checked against a real install; the notes on each say what
/// was found, because several of them contradict the tool's published docs.
pub const AGENTS: &[AgentSpec] = &[
    // GitHub Copilot CLI. The default, and the only row whose behaviour is
    // load-bearing for existing users: `copilot --acp --stdio`, with
    // `--deny-tool=<name>` for each refusal. `--stdio` is real but hidden --
    // it does not appear in `--help`.
    AgentSpec {
        id: "copilot",
        display_name: "GitHub Copilot",
        windows_names: &["copilot.exe", "copilot.cmd", "copilot.bat", "copilot"],
        unix_names: &["copilot"],
        acp_args: &["--acp", "--stdio"],
        version_args: &["version"],
        denial: Denial::LaunchFlags {
            deny_flag: "--deny-tool",
        },
        // Verified against `copilot help permissions` on 1.0.76: the
        // vocabulary is permission KINDS -- shell, write, url, and MCP server
        // names -- not file-operation names. A wrong name here is silently
        // ignored rather than rejected, so it would look like it worked while
        // filtering nothing.
        tool_names: DeniableTools {
            shell: &["shell"],
            network: &["url"],
            write: &["write"],
        },
        homepage_url: "https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli",
        install_hint: "npm install -g @github/copilot",
    },
    // Gemini CLI. `--acp` is the current flag; `--experimental-acp` still
    // works but is deprecated in favour of it.
    //
    // No true per-tool denial. `--allowed-tools` is deprecated and only skips
    // the confirmation prompt rather than restricting anything, so it is not
    // used at all. The closest real bound is `--approval-mode=plan`, a
    // whole-session read-only mode.
    AgentSpec {
        id: "gemini",
        display_name: "Gemini CLI",
        windows_names: &["gemini.exe", "gemini.cmd", "gemini.bat", "gemini"],
        unix_names: &["gemini"],
        acp_args: &["--acp"],
        version_args: &["--version"],
        denial: Denial::ReadOnlyMode {
            flag: "--approval-mode=plan",
        },
        tool_names: NO_TOOL_NAMES,
        homepage_url: "https://github.com/google-gemini/gemini-cli#quickstart",
        install_hint: "npm install -g @google/gemini-cli",
    },
    // Claude Code, reached through an adapter rather than directly: Claude
    // Code itself has no ACP mode. The adapter is npm
    // `@agentclientprotocol/claude-agent-acp`, whose binary is
    // `claude-agent-acp`.
    //
    // Worth knowing when reading install instructions elsewhere:
    // `@zed-industries/claude-code-acp` and `@zed-industries/claude-agent-acp`
    // are both npm-deprecated renames of the same package, so a machine may
    // have the same binary from any of the three.
    //
    // Denial travels in the protocol rather than on the command line, which
    // is why this row exists as its own capability shape.
    AgentSpec {
        id: "claude",
        display_name: "Claude Code",
        windows_names: &[
            "claude-agent-acp.exe",
            "claude-agent-acp.cmd",
            "claude-agent-acp.bat",
            "claude-agent-acp",
        ],
        unix_names: &["claude-agent-acp"],
        acp_args: &[],
        version_args: &["--version"],
        denial: Denial::SessionMeta,
        // Claude Code's own tool names, as they appear in `disallowedTools`.
        // The adapter matches on EXACT names, so these are spelled the way
        // Claude Code spells them rather than the way GitWyrm does -- and
        // every name for a capability has to be listed, because a name left
        // out is a tool that stays available.
        //
        // `Write` alone would not have been enough: Claude Code edits files
        // through `Edit`, `MultiEdit` and `NotebookEdit` as well, so denying
        // only `Write` would leave a read-only session able to change files
        // by another name. Likewise `WebFetch` and `WebSearch` are separate
        // tools. `Bash` covers shell; `BashOutput` and `KillShell` only
        // manage a shell that `Bash` alone can start.
        tool_names: DeniableTools {
            shell: &["Bash", "BashOutput", "KillShell"],
            network: &["WebFetch", "WebSearch"],
            write: &["Write", "Edit", "MultiEdit", "NotebookEdit"],
        },
        // The adapter, not Claude Code itself: Claude Code has no ACP mode, so
        // this is the package that provides one. Someone who installs Claude
        // Code alone still ends up with nothing GitWyrm can drive.
        homepage_url: "https://www.npmjs.com/package/@agentclientprotocol/claude-agent-acp",
        install_hint: "npm install -g @agentclientprotocol/claude-agent-acp",
    },
    // opencode. ACP is a SUBCOMMAND (`opencode acp`), not a flag.
    //
    // It has no tool-restriction flags of any kind. That is not an oversight
    // in this table -- there is nothing to put here. The consequence is
    // enforced in `select::choose`: opencode is refused for read-only work,
    // because GitWyrm cannot promise a session will not change anything when
    // the tool cannot be told not to.
    AgentSpec {
        id: "opencode",
        display_name: "opencode",
        windows_names: &["opencode.exe", "opencode.cmd", "opencode.bat", "opencode"],
        unix_names: &["opencode"],
        acp_args: &["acp"],
        version_args: &["--version"],
        denial: Denial::None,
        tool_names: NO_TOOL_NAMES,
        homepage_url: "https://opencode.ai/docs/",
        install_hint: "npm install -g opencode-ai",
    },
];

/// The agent used when nothing else is chosen. Copilot, unchanged.
pub const DEFAULT_AGENT_ID: &str = "copilot";

/// Looks up one agent by its stable id, case-insensitively.
///
/// `None` means this build does not support that name -- the caller must
/// refuse rather than falling back to the default, which is the whole point of
/// `policy::PolicyRefusal::UnsupportedProvider`.
pub fn find(id: &str) -> Option<&'static AgentSpec> {
    AGENTS.iter().find(|a| a.id.eq_ignore_ascii_case(id))
}

/// The default agent's row. Present by construction; falls back to the first
/// row rather than panicking if the id above is ever edited out of step.
pub fn default_agent() -> &'static AgentSpec {
    find(DEFAULT_AGENT_ID).unwrap_or(&AGENTS[0])
}

/// Directories worth looking in when PATH has not caught up.
///
/// Common right after an install, before the user has opened a new shell.
/// Shared by every agent rather than being per-row, because these are the
/// places npm and the native installers write to in general, not places any
/// one tool is special about.
pub fn known_locations() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = home_dir() {
        out.push(home.join(".copilot").join("bin"));
        out.push(home.join(".local").join("bin"));
        out.push(home.join(".bun").join("bin"));
        if cfg!(windows) {
            out.push(
                home.join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("copilot"),
            );
            out.push(home.join("AppData").join("Roaming").join("npm"));
        }
    }
    if !cfg!(windows) {
        out.push(PathBuf::from("/usr/local/bin"));
        out.push(PathBuf::from("/opt/homebrew/bin"));
    }
    out
}

/// `USERPROFILE` then `HOME`, matching how the rest of the codebase resolves
/// it (see `git::ssh` and `git::identity`).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_has_a_distinct_lower_case_id() {
        for spec in AGENTS {
            assert_eq!(spec.id, spec.id.to_lowercase(), "{} must have a lower-case id", spec.id);
            assert!(!spec.id.contains(' '), "{} must not contain a space", spec.id);
            assert_eq!(
                AGENTS.iter().filter(|o| o.id == spec.id).count(),
                1,
                "{} appears more than once",
                spec.id
            );
        }
    }

    #[test]
    fn the_default_agent_is_copilot_and_is_in_the_table() {
        assert_eq!(default_agent().id, "copilot");
        assert!(find("copilot").is_some());
    }

    #[test]
    fn looking_up_an_agent_ignores_case() {
        for name in ["copilot", "Copilot", "COPILOT"] {
            assert_eq!(find(name).map(|a| a.id), Some("copilot"));
        }
    }

    #[test]
    fn looking_up_an_unknown_agent_finds_nothing_rather_than_the_default() {
        // A silent fall-back to Copilot is the exact bug
        // `PolicyRefusal::UnsupportedProvider` exists to close.
        for name in ["gpt-4", "cop1lot", "", "claude-code"] {
            assert!(find(name).is_none(), "{name:?} must not resolve");
        }
    }

    /// Windows npm shims are the case that broke discovery once already: the
    /// documented install writes a `.cmd` and no `.exe`, so a search for the
    /// `.exe` alone reports "not installed" on a working machine.
    #[test]
    fn every_agent_looks_for_the_windows_npm_shim_not_just_an_exe() {
        for spec in AGENTS {
            let names = spec.windows_names;
            let base = spec.unix_names[0];
            let cmd = format!("{base}.cmd");
            let exe = format!("{base}.exe");
            assert!(
                names.contains(&cmd.as_str()),
                "{} must search for the npm .cmd shim",
                spec.id
            );
            assert!(
                names.contains(&exe.as_str()),
                "{} must search for a native .exe",
                spec.id
            );
            assert!(names.contains(&base), "{} must also search for the bare name", spec.id);
            assert!(
                names.iter().position(|n| *n == cmd) < names.iter().position(|n| *n == base),
                "{} must prefer the .cmd shim over the bare shell script",
                spec.id
            );
        }
    }

    #[test]
    fn candidate_names_pick_the_right_list_for_this_platform() {
        let copilot = find("copilot").expect("copilot is in the table");
        if cfg!(windows) {
            assert_eq!(copilot.candidate_names(), copilot.windows_names);
            assert!(copilot.candidate_names().contains(&"copilot.cmd"));
        } else {
            assert_eq!(copilot.candidate_names(), &["copilot"]);
        }
    }

    /// The exact launch lines, checked against real binaries. A change here
    /// starts the wrong process or starts it in the wrong mode, and neither
    /// fails in a way that reads as a launch-line problem.
    #[test]
    fn each_agent_starts_its_acp_server_with_the_arguments_that_actually_work() {
        let cases: &[(&str, &[&str])] = &[
            // `--stdio` is a real but hidden flag; it is not in `--help`.
            ("copilot", &["--acp", "--stdio"]),
            // `--experimental-acp` still exists but is deprecated in favour
            // of `--acp`.
            ("gemini", &["--acp"]),
            // The adapter IS the ACP server, so it takes no mode argument.
            ("claude", &[]),
            // A subcommand, not a flag.
            ("opencode", &["acp"]),
        ];
        for (id, expected) in cases {
            let spec = find(id).unwrap_or_else(|| panic!("{id} must be in the table"));
            assert_eq!(spec.acp_args, *expected, "{id} launch arguments");
        }
    }

    #[test]
    fn copilot_denies_each_tool_with_its_own_flag() {
        let copilot = find("copilot").expect("copilot is in the table");
        let args = copilot.launch_args(&["shell", "url", "write"]);
        assert_eq!(
            args,
            vec![
                "--acp",
                "--stdio",
                "--deny-tool=shell",
                "--deny-tool=url",
                "--deny-tool=write",
            ]
        );
    }

    #[test]
    fn copilot_launched_with_nothing_denied_carries_no_deny_flags() {
        let copilot = find("copilot").expect("copilot is in the table");
        assert_eq!(copilot.launch_args(&[]), vec!["--acp", "--stdio"]);
    }

    /// Gemini cannot express a partial denial, so any denial at all puts the
    /// whole session in read-only mode. Doing less would be the unsafe
    /// reading.
    #[test]
    fn gemini_turns_a_write_denial_into_whole_session_read_only_mode() {
        let gemini = find("gemini").expect("gemini is in the table");
        assert_eq!(gemini.launch_args(&["write"]), vec!["--acp", "--approval-mode=plan"]);
        assert_eq!(gemini.launch_args(&[]), vec!["--acp"]);
    }

    /// A Fix run denies `shell` and `url` but NOT `write`, and must still be
    /// able to change files.
    ///
    /// This is the case the old test missed: it only checked the empty-denial
    /// input, which production never produces (`ALWAYS_DENIED_TOOLS` puts
    /// `shell` and `url` on every run), so a rule of "any denial turns on
    /// read-only mode" looked correct while silently making every Gemini Fix
    /// session unable to write.
    #[test]
    fn a_write_capable_run_is_not_forced_into_read_only_mode() {
        let gemini = find("gemini").expect("gemini is in the table");
        let args = gemini.launch_args(&["shell", "url"]);
        assert!(
            !args.iter().any(|a| a.contains("approval-mode")),
            "a run allowed to write must not launch in read-only mode, got {args:?}"
        );
    }

    /// The Claude adapter's denial is in the protocol, so nothing about it
    /// appears on the command line.
    #[test]
    fn the_claude_adapter_denies_in_session_meta_not_on_the_command_line() {
        let claude = find("claude").expect("claude is in the table");
        assert!(
            claude.launch_args(&["shell", "url", "write"]).is_empty(),
            "denial must not leak onto the adapter's command line"
        );
        let meta = claude
            .session_meta(&["shell", "url", "write"])
            .expect("a denial must produce a _meta object");
        let disallowed = meta["claudeCode"]["options"]["disallowedTools"]
            .as_array()
            .expect("disallowedTools must be a list");
        let names: Vec<&str> = disallowed.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"WebFetch"));
        assert!(names.contains(&"Write"));
    }

    #[test]
    fn an_agent_that_denies_on_the_command_line_sends_no_session_meta() {
        for id in ["copilot", "gemini", "opencode"] {
            let spec = find(id).expect("in the table");
            assert!(
                spec.session_meta(&["write"]).is_none(),
                "{id} must not send a _meta denial"
            );
        }
    }

    #[test]
    fn opencode_has_no_way_to_deny_anything() {
        let opencode = find("opencode").expect("opencode is in the table");
        assert_eq!(opencode.denial, Denial::None);
        // Denying everything still produces the bare launch line, because
        // there is no flag to add.
        assert_eq!(opencode.launch_args(&["shell", "url", "write"]), vec!["acp"]);
    }

    /// THE safety fact the refusal in `select::choose` rests on: exactly the
    /// agents with no denial capability cannot be trusted with read-only work.
    #[test]
    fn only_an_agent_with_no_denial_capability_cannot_guarantee_read_only() {
        for spec in AGENTS {
            assert_eq!(
                spec.can_guarantee_read_only(),
                spec.denial != Denial::None,
                "{} read-only guarantee must follow its denial capability",
                spec.id
            );
        }
        assert!(!find("opencode").unwrap().can_guarantee_read_only());
        assert!(find("copilot").unwrap().can_guarantee_read_only());
        assert!(find("gemini").unwrap().can_guarantee_read_only());
        assert!(find("claude").unwrap().can_guarantee_read_only());
    }

    /// A row whose denial names tools must name all three GitWyrm gates.
    /// Forgetting the write name is the difference between a read-only
    /// session and one that can edit the user's files, and it would not fail
    /// loudly anywhere else.
    #[test]
    fn an_agent_that_denies_by_name_names_all_three_gated_tools() {
        for spec in AGENTS {
            let names_tools = matches!(spec.denial, Denial::LaunchFlags { .. } | Denial::SessionMeta);
            if names_tools {
                assert!(!spec.tool_names.shell.is_empty(), "{} shell name", spec.id);
                assert!(!spec.tool_names.network.is_empty(), "{} network name", spec.id);
                assert!(!spec.tool_names.write.is_empty(), "{} write name", spec.id);
            }
        }
    }

    /// Claude Code writes files through more than one tool, and
    /// `disallowedTools` matches on exact names. Denying only `Write` would
    /// leave `Edit`/`MultiEdit`/`NotebookEdit` available to a session that
    /// was promised it could not change anything -- the exact hole this list
    /// exists to close, and one nothing else would catch.
    #[test]
    fn claudes_write_denial_covers_every_tool_that_edits_a_file() {
        let claude = find("claude").expect("the Claude row must exist");
        let meta = claude
            .session_meta(&["write"])
            .expect("a write denial must produce a session _meta");
        let denied = meta["claudeCode"]["options"]["disallowedTools"]
            .as_array()
            .expect("disallowedTools must be a list")
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        for tool in ["Write", "Edit", "MultiEdit", "NotebookEdit"] {
            assert!(
                denied.contains(&tool),
                "{tool} can change a file and must be denied, got {denied:?}"
            );
        }
    }

    /// The same reasoning for the other two gates: one name per capability
    /// was never enough for a tool that spells a capability several ways.
    #[test]
    fn claudes_shell_and_network_denials_cover_their_aliases() {
        let claude = find("claude").expect("the Claude row must exist");
        let meta = claude
            .session_meta(&["shell", "url"])
            .expect("a denial must produce a session _meta");
        let denied = meta["claudeCode"]["options"]["disallowedTools"]
            .as_array()
            .expect("disallowedTools must be a list")
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        for tool in ["Bash", "WebFetch", "WebSearch"] {
            assert!(denied.contains(&tool), "{tool} must be denied, got {denied:?}");
        }
    }

    /// A tool that names several aliases must emit a separate deny flag for
    /// each: one flag carrying a comma-joined list would be read by the CLI
    /// as a single tool with a very strange name, and would filter nothing.
    #[test]
    fn each_denied_alias_becomes_its_own_launch_flag() {
        let copilot = find("copilot").expect("the Copilot row must exist");
        let args = copilot.launch_args(&["shell", "url", "write"]);
        for expected in ["--deny-tool=shell", "--deny-tool=url", "--deny-tool=write"] {
            assert!(args.contains(&expected.to_string()), "missing {expected} in {args:?}");
        }
        assert!(
            !args.iter().any(|a| a.contains(',')),
            "a deny flag must name one tool, got {args:?}"
        );
    }

    #[test]
    fn every_agent_knows_how_to_be_asked_its_version() {
        for spec in AGENTS {
            assert!(!spec.version_args.is_empty(), "{} must have a version probe", spec.id);
        }
    }
}
