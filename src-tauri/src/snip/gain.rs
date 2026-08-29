//! Reading the savings `snip` has recorded, via `snip gain --json`.
//!
//! # Why the field names look inconsistent
//!
//! They are inconsistent, and faithfully so. `snip` builds its JSON from a map
//! with three hand-written keys -- `summary`, `daily`, `by_command` -- but the
//! structs it puts in that map carry no JSON tags at all, so Go serializes
//! their fields under the exported Go names: `TotalSaved`, `AvgSavings`, `Day`.
//! Renaming them here to something tidier would simply stop them matching, so
//! the outer keys stay snake_case and the inner ones are read as PascalCase.
//!
//! # Why so much of it is optional
//!
//! `snip` discards the errors from its two list queries and marshals whatever
//! it has. A database that exists but cannot be read therefore produces
//! `"daily": null` rather than a failure, and there are no `omitempty` tags to
//! turn a missing list into an absent key. Both lists are read as `Option`, and
//! the summary too, since a nil summary marshals to `null` the same way.

use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use crate::git::shell::CREATE_NO_WINDOW;

use super::detect::{detect, SnipState};

/// Reading a small SQLite file should be near-instant; this only stops a wedged
/// process from blocking the caller forever.
const REPORT_TIMEOUT: Duration = Duration::from_secs(10);

/// What `snip` prints when its `gain` command was compiled out.
///
/// A `-tags lite` build has no SQLite in it, so `gain` exits 1 with this text
/// rather than reporting anything. Matched loosely (lowercased, on the
/// distinctive half of the sentence) because the wording is the CLI's to change
/// and the exact phrasing carries no meaning we depend on.
const LITE_BUILD_MARKER: &str = "requires full build";

/// Totals across everything `snip` has recorded.
///
/// PascalCase on the wire: these are untagged Go struct fields.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub struct SnipSummary {
    /// How many commands `snip` has filtered.
    pub total_commands: i64,
    /// Tokens saved in total, summed over those commands.
    pub total_saved: i64,
    /// Mean percentage saved per command.
    pub avg_savings: f64,
    /// Total time those commands spent running, in milliseconds.
    pub total_time_ms: i64,
}

/// One day's worth of savings.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub struct SnipDaily {
    /// The day, as `snip` formatted it. Kept as text: it is only ever shown,
    /// never compared, and parsing it would invent a format contract that the
    /// CLI has not promised.
    pub day: String,
    pub commands: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub saved_tokens: i64,
    pub avg_savings: f64,
}

/// Savings attributed to one command.
///
/// `snip` returns at most ten of these -- the limit is hardcoded in its query,
/// not something we ask for -- so this is a top-ten list, never an exhaustive
/// one. Anything built on it should say "top commands", not "all commands".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub struct SnipByCommand {
    pub command: String,
    pub count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub saved_tokens: i64,
    pub avg_savings: f64,
}

/// The whole report, exactly as `snip gain --json` shapes it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SnipGainReport {
    pub summary: Option<SnipSummary>,
    pub daily: Option<Vec<SnipDaily>>,
    pub by_command: Option<Vec<SnipByCommand>>,
}

/// What came of asking for the report.
///
/// A typed outcome rather than a `Result` with a string in it: every one of
/// these is a state the UI should describe differently, and only the last is
/// something that went wrong. Flattening them into an error would leave the
/// screen unable to tell "you have not used it yet" from "it is broken".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SnipGainOutcome {
    /// The report was read.
    Available { report: SnipGainReport },
    /// No `snip` on this machine, so there is nothing to report on.
    NotInstalled,
    /// `snip` is installed but has not recorded anything yet -- a fresh install,
    /// or one that has not wrapped a command. Not a failure, and the difference
    /// matters: the answer here is "use it once", not "something is wrong".
    NoData,
    /// This `snip` was built without the tracking database, so it cannot report
    /// savings at all. A build-time choice, permanent for this binary, and only
    /// fixable by installing a full build -- which is why it is its own state
    /// rather than a generic failure the user would keep retrying.
    LiteBuild,
    /// Something else went wrong. `detail` is the CLI's own text, for the log
    /// and for a diagnostics view; the UI wraps it in plain language.
    Failed { detail: String },
}

/// Runs `snip gain --json` and interprets what came back.
pub fn gain() -> SnipGainOutcome {
    let SnipState::Ready { path, .. } = detect() else {
        return SnipGainOutcome::NotInstalled;
    };

    let mut cmd = Command::new(&path);
    cmd.args(["gain", "--json"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    // As in the version probe: `output()` cannot time out on its own, so it runs
    // on a thread the caller is free to abandon.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });

    let out = match rx.recv_timeout(REPORT_TIMEOUT) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            log::warn!("snip: could not run `gain`: {e}");
            return SnipGainOutcome::Failed {
                detail: e.to_string(),
            };
        }
        Err(_) => {
            log::warn!("snip: `gain` timed out after {REPORT_TIMEOUT:?}");
            return SnipGainOutcome::Failed {
                detail: format!("snip did not answer within {} seconds", REPORT_TIMEOUT.as_secs()),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        // The lite-build refusal is the one non-zero exit with a specific
        // meaning, and it can land on either stream depending on how the CLI
        // was invoked, so both are checked.
        if is_lite_build(&stdout) || is_lite_build(&stderr) {
            log::info!("snip: this build has no tracking database, so `gain` cannot report");
            return SnipGainOutcome::LiteBuild;
        }
        let detail = pick_detail(&stderr, &stdout);
        log::warn!("snip: `gain` failed: {detail}");
        return SnipGainOutcome::Failed { detail };
    }

    interpret_report(&stdout)
}

/// Turns the CLI's stdout into an outcome.
///
/// Split out from [`gain`] so the JSON shapes can be tested without a `snip`
/// install standing by.
fn interpret_report(stdout: &str) -> SnipGainOutcome {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        // A clean exit with nothing printed is not a failure worth alarming
        // anyone over; there is simply nothing recorded.
        return SnipGainOutcome::NoData;
    }

    match serde_json::from_str::<SnipGainReport>(trimmed) {
        Ok(report) => {
            if is_empty(&report) {
                SnipGainOutcome::NoData
            } else {
                SnipGainOutcome::Available { report }
            }
        }
        Err(e) => {
            log::warn!("snip: could not read the `gain` report: {e}");
            SnipGainOutcome::Failed {
                detail: e.to_string(),
            }
        }
    }
}

/// True when the report carries nothing worth showing.
///
/// A report can parse perfectly and still be empty: a fresh database answers
/// with a zero summary and null lists. Treating that as `Available` would put
/// a table of nothing on screen and leave the user guessing whether it was
/// broken or simply unused.
fn is_empty(report: &SnipGainReport) -> bool {
    let no_commands = report
        .summary
        .as_ref()
        .is_none_or(|s| s.total_commands == 0);
    let no_daily = report.daily.as_ref().is_none_or(|d| d.is_empty());
    let no_by_command = report.by_command.as_ref().is_none_or(|c| c.is_empty());
    no_commands && no_daily && no_by_command
}

fn is_lite_build(text: &str) -> bool {
    text.to_lowercase().contains(LITE_BUILD_MARKER)
}

/// Prefers stderr for the failure text, falling back to stdout, then to a
/// plain sentence when the CLI said nothing at all.
fn pick_detail(stderr: &str, stdout: &str) -> String {
    let err = stderr.trim();
    if !err.is_empty() {
        return err.to_string();
    }
    let out = stdout.trim();
    if !out.is_empty() {
        return out.to_string();
    }
    "snip stopped without saying why".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full report in the shape `snip gain --json` actually emits: snake_case
    /// outer keys, PascalCase inner fields.
    const FULL_REPORT: &str = r#"{
        "summary": {
            "TotalCommands": 42,
            "TotalSaved": 128000,
            "AvgSavings": 87.5,
            "TotalTimeMs": 91234
        },
        "daily": [
            {
                "Day": "2026-08-27",
                "Commands": 12,
                "InputTokens": 40000,
                "OutputTokens": 5000,
                "SavedTokens": 35000,
                "AvgSavings": 87.5
            }
        ],
        "by_command": [
            {
                "Command": "npm test",
                "Count": 9,
                "InputTokens": 30000,
                "OutputTokens": 2000,
                "SavedTokens": 28000,
                "AvgSavings": 93.3
            }
        ]
    }"#;

    #[test]
    fn the_outer_keys_are_snake_case_and_the_inner_fields_are_pascal_case() {
        // The mismatch is the CLI's, not ours: it writes the three outer keys by
        // hand and lets Go name the inner fields. Reading them any other way
        // silently produces an empty report.
        let outcome = interpret_report(FULL_REPORT);
        let SnipGainOutcome::Available { report } = outcome else {
            panic!("a full report must be available, got {outcome:?}");
        };
        let summary = report.summary.expect("summary must parse");
        assert_eq!(summary.total_commands, 42);
        assert_eq!(summary.total_saved, 128_000);
        assert_eq!(summary.avg_savings, 87.5);
        assert_eq!(summary.total_time_ms, 91_234);

        let daily = report.daily.expect("daily must parse");
        assert_eq!(daily[0].day, "2026-08-27");
        assert_eq!(daily[0].saved_tokens, 35_000);

        let by_command = report.by_command.expect("by_command must parse");
        assert_eq!(by_command[0].command, "npm test");
        assert_eq!(by_command[0].count, 9);
    }

    #[test]
    fn null_lists_are_read_as_absent_rather_than_failing() {
        // snip throws away the errors from its two list queries and marshals
        // what is left, so a readable summary alongside null lists is a shape
        // that really occurs. Refusing to parse it would report a broken tool.
        let json = r#"{
            "summary": {"TotalCommands": 5, "TotalSaved": 100, "AvgSavings": 50.0, "TotalTimeMs": 10},
            "daily": null,
            "by_command": null
        }"#;
        let outcome = interpret_report(json);
        let SnipGainOutcome::Available { report } = outcome else {
            panic!("a summary with null lists is still a usable report, got {outcome:?}");
        };
        assert!(report.daily.is_none());
        assert!(report.by_command.is_none());
        assert_eq!(report.summary.expect("summary").total_commands, 5);
    }

    #[test]
    fn a_null_summary_does_not_fail_the_parse() {
        let json = r#"{"summary": null, "daily": null, "by_command": null}"#;
        assert!(matches!(interpret_report(json), SnipGainOutcome::NoData));
    }

    #[test]
    fn a_fresh_install_with_nothing_recorded_reads_as_no_data() {
        // Zero commands and empty lists parse fine but are worth nothing on
        // screen. "You have not used it yet" is a different message from "it
        // is broken", and the user needs the first one.
        let json = r#"{
            "summary": {"TotalCommands": 0, "TotalSaved": 0, "AvgSavings": 0.0, "TotalTimeMs": 0},
            "daily": [],
            "by_command": []
        }"#;
        assert!(matches!(interpret_report(json), SnipGainOutcome::NoData));
    }

    #[test]
    fn an_empty_stdout_is_no_data_not_a_failure() {
        assert!(matches!(interpret_report("   \n"), SnipGainOutcome::NoData));
    }

    #[test]
    fn unreadable_output_is_reported_as_a_failure() {
        let outcome = interpret_report("this is not json");
        assert!(
            matches!(outcome, SnipGainOutcome::Failed { .. }),
            "got {outcome:?}"
        );
    }

    #[test]
    fn a_lite_build_is_reported_as_its_own_state_not_a_failure() {
        // A `-tags lite` build compiles the tracking database out entirely, so
        // `gain` exits 1 with this line. It is permanent for that binary, so
        // showing it as a generic failure would invite a retry that can never
        // work.
        assert!(is_lite_build(
            "gain requires full build (this binary was built with -tags lite)"
        ));
    }

    #[test]
    fn the_lite_build_check_ignores_letter_case() {
        assert!(is_lite_build("GAIN REQUIRES FULL BUILD"));
    }

    #[test]
    fn an_ordinary_failure_is_not_mistaken_for_a_lite_build() {
        assert!(!is_lite_build("unable to open tracking database"));
    }

    #[test]
    fn failure_text_prefers_stderr_but_falls_back_to_stdout() {
        assert_eq!(pick_detail("bad things", "ignored"), "bad things");
        assert_eq!(pick_detail("  ", "on stdout"), "on stdout");
        assert_eq!(pick_detail("", ""), "snip stopped without saying why");
    }
}
