//! Checking whether a finished run actually did the job.
//!
//! # Why this is adversarial rather than a completion check
//!
//! An agent that has ticked every box on a task list has not necessarily done
//! the work. Two failure modes are common enough to design around:
//!
//! - **Writing to the letter of the spec.** The spec says "add a retry"; the
//!   agent adds a `retry` parameter nothing reads. Every stated requirement is
//!   technically present and the feature does not exist.
//! - **Cutting corners to reach green.** A test that asserts `true`, a
//!   function that returns a hardcoded value, an error path that swallows and
//!   moves on. Checks pass, and the code would not survive a real user.
//!
//! So the auditor is not asked "is this done?" -- an agent that cut corners
//! will say yes. It is asked to find what is wrong, with the spec as evidence
//! of what was *meant* rather than as a checklist to satisfy literally.
//!
//! # The spec informs; the auditor decides
//!
//! Specs are loose. They are written before the work, by someone who did not
//! yet know what the work would involve, and they routinely under-describe the
//! thing they are asking for. A run whose boxes are all ticked can still be
//! hollow, and a run that missed a stated detail can still be right.
//!
//! That is why [`Verdict::Hollow`] exists as a state distinct from both done
//! and blocked: it is specifically "this looks finished and is not", which is
//! the case a completion check cannot express.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What the auditor concluded about a finished run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Verdict {
    /// The work is real and does what was asked.
    Passed { note: String },
    /// It looks finished but is not: stubs, tests that assert nothing, a
    /// requirement satisfied in name only.
    ///
    /// `reasons` are what goes back to the agent, so each one has to be
    /// specific enough to act on. "Needs improvement" is not a reason.
    Hollow { reasons: Vec<String> },
    /// The run could not finish for a reason no amount of retrying fixes --
    /// a missing credential, a service that is down, a decision only a person
    /// can make.
    Blocked { reason: String },
    /// The audit itself did not produce a usable answer.
    ///
    /// Deliberately not a rejection. An auditor that failed to answer knows
    /// nothing about the work, and treating silence as "hollow" would send an
    /// agent back to fix problems nobody found.
    Unavailable { detail: String },
}

impl Verdict {
    /// Whether this verdict should send the run back to keep working.
    pub fn wants_another_pass(&self) -> bool {
        matches!(self, Verdict::Hollow { .. })
    }

    /// One plain sentence for the transcript.
    pub fn summary(&self) -> String {
        match self {
            Verdict::Passed { note } if note.trim().is_empty() => {
                "Checked the work over: it does what was asked.".into()
            }
            Verdict::Passed { note } => format!("Checked the work over: {note}"),
            Verdict::Hollow { reasons } => {
                let n = reasons.len();
                if n == 1 {
                    "Found one thing that is not really finished; sending it back.".into()
                } else {
                    format!("Found {n} things that are not really finished; sending it back.")
                }
            }
            Verdict::Blocked { reason } => format!("This cannot go further without you: {reason}"),
            Verdict::Unavailable { .. } => {
                "Could not double-check this work, so it is being left as it is.".into()
            }
        }
    }
}

/// How many times one run may be sent back before it is handed over anyway.
///
/// Two, not one: the first pass often fixes the obvious thing and reveals the
/// next. Not unbounded, because an auditor and an agent that disagree about
/// what "done" means would otherwise spend the whole budget arguing, and the
/// person waiting learns nothing from watching it.
pub const MAX_CORRECTION_PASSES: u32 = 2;

/// How much of the diff the auditor is shown.
///
/// A cap exists because the diff is the expensive part of the prompt and a
/// large refactor could be enormous. It is generous rather than tight: the
/// point of reading the diff at all is to catch the stub nobody would notice,
/// and a cap so low that it truncates before reaching the interesting file
/// buys nothing.
pub const MAX_DIFF_BYTES: usize = 120_000;

/// What the auditor is given to judge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvidence {
    /// The spec, rendered the same way the working agent saw it. Empty when
    /// the run had no OpenSpec source.
    pub spec: String,
    /// What the agent said it did.
    pub agent_summary: String,
    /// The actual change, as unified diff text.
    pub diff: String,
    /// Whether `diff` had to be cut short.
    pub diff_truncated: bool,
    /// Files touched, with their status letters.
    pub changed_paths: Vec<String>,
    /// Project checks that ran, and how they went.
    pub checks: Vec<String>,
}

/// The prompt that asks for an adversarial read.
///
/// Written to fight the model's own agreeableness. Asked "is this done?", a
/// model reads a plausible diff and says yes; the instruction has to make
/// finding nothing an active claim rather than the path of least resistance.
///
/// The reply is a fenced JSON block for the same reason
/// `plan_proposal::plan_mode_instruction` uses one: prose has to be guessed
/// at, and a guess about whether work passed is not a guess worth making.
pub fn audit_prompt(evidence: &AuditEvidence) -> String {
    let mut p = String::new();

    p.push_str(
        "You are reviewing work another agent just finished. It believes it is done.\n\
         Your job is to find what is wrong with it.\n\n\
         Assume two things until the code proves otherwise:\n\
         - the request it was given was looser than it looks, and\n\
         - the agent may have done the smallest thing that looks like success.\n\n\
         Specifically look for: functions that return a fixed value instead of \
         doing the work; tests that cannot fail, or that assert something \
         trivially true; error paths that swallow a problem; a requirement met \
         in name only, like a parameter that is accepted and never used; and \
         anything that would break the first time a real person used it in a \
         way the tests do not cover.\n\n\
         Judge whether the work is REAL, not whether every stated item was \
         ticked. A request written before the work started is evidence of what \
         was wanted, not a checklist. Work can satisfy every line of it and \
         still be hollow, and it can miss a stated detail and still be right.\n\n",
    );

    if !evidence.spec.trim().is_empty() {
        p.push_str("--- WHAT WAS ASKED FOR ---\n");
        p.push_str(evidence.spec.trim());
        p.push_str("\n\n");
    } else {
        p.push_str(
            "--- WHAT WAS ASKED FOR ---\n\
             Nothing written down. Judge the change on its own terms.\n\n",
        );
    }

    p.push_str("--- WHAT THE AGENT SAYS IT DID ---\n");
    if evidence.agent_summary.trim().is_empty() {
        p.push_str("It did not say.\n\n");
    } else {
        p.push_str(evidence.agent_summary.trim());
        p.push_str("\n\n");
    }

    p.push_str("--- FILES TOUCHED ---\n");
    if evidence.changed_paths.is_empty() {
        p.push_str(
            "None. A run that changed no files has almost certainly not done \
             the work, unless it was only ever meant to answer a question.\n\n",
        );
    } else {
        for path in &evidence.changed_paths {
            p.push_str(&format!("{path}\n"));
        }
        p.push('\n');
    }

    if !evidence.checks.is_empty() {
        p.push_str("--- CHECKS THAT RAN ---\n");
        for check in &evidence.checks {
            p.push_str(&format!("{check}\n"));
        }
        p.push_str(
            "\nPassing checks are weak evidence. A test written alongside the \
             code it tests can be written to pass.\n\n",
        );
    }

    p.push_str("--- THE ACTUAL CHANGE ---\n");
    if evidence.diff.trim().is_empty() {
        p.push_str("(no diff was available)\n");
    } else {
        p.push_str(evidence.diff.trim());
        p.push('\n');
        if evidence.diff_truncated {
            p.push_str(
                "\n(the change was too large to show in full and was cut off here; \
                 judge what you can see and say so if that is not enough)\n",
            );
        }
    }

    p.push_str(
        "\n--- ANSWER ---\n\
         Reply with one fenced block and nothing else:\n\n\
         ```json\n\
         { \"verdict\": \"passed\" | \"hollow\" | \"blocked\",\n\
         \"note\": \"one sentence, only when passed\",\n\
         \"reasons\": [\"one specific, actionable problem\", \"...\"],\n\
         \"blockedReason\": \"only when blocked\" }\n\
         ```\n\n\
         Use \"hollow\" when the work looks finished but is not. Every reason \
         must name a file and say what is wrong with it, specifically enough \
         that someone could fix it without asking you what you meant. \
         \"Needs more work\" is not a reason.\n\
         Use \"blocked\" only for something no amount of further work fixes: a \
         missing credential, a service that is down, a decision only a person \
         can make.\n\
         Use \"passed\" when the work is real. Do not invent problems to look \
         thorough -- a wrong rejection costs someone real time.\n",
    );

    p
}

/// The fenced language the reply is expected in.
const FENCE_LANGUAGE: &str = "json";

/// Reads the auditor's reply.
///
/// A reply this cannot understand becomes [`Verdict::Unavailable`], never a
/// rejection: an unreadable answer means the audit did not happen, and sending
/// an agent back over problems nobody identified would be worse than letting
/// the work through for a person to look at.
pub fn parse_verdict(reply: &str) -> Verdict {
    let Some(block) = fenced_json(reply) else {
        return Verdict::Unavailable {
            detail: "the check did not answer in a way GitWyrm could read".into(),
        };
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&block) else {
        return Verdict::Unavailable {
            detail: "the check's answer was not valid JSON".into(),
        };
    };

    match value.get("verdict").and_then(|v| v.as_str()) {
        Some("passed") => Verdict::Passed {
            note: value
                .get("note")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string(),
        },
        Some("hollow") => {
            let reasons: Vec<String> = value
                .get("reasons")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            // A rejection with no reasons cannot be acted on, and sending an
            // agent back with nothing to fix wastes a whole pass. Treated as
            // no answer at all.
            if reasons.is_empty() {
                Verdict::Unavailable {
                    detail: "the check said the work was unfinished but did not say why".into(),
                }
            } else {
                Verdict::Hollow { reasons }
            }
        }
        Some("blocked") => Verdict::Blocked {
            reason: value
                .get("blockedReason")
                .or_else(|| value.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("it did not say what is in the way")
                .trim()
                .to_string(),
        },
        _ => Verdict::Unavailable {
            detail: "the check did not say whether the work was finished".into(),
        },
    }
}

/// The contents of the first ```json fence, or the first fence of any kind.
fn fenced_json(reply: &str) -> Option<String> {
    let open = reply
        .find(&format!("```{FENCE_LANGUAGE}"))
        .map(|i| i + 3 + FENCE_LANGUAGE.len())
        .or_else(|| reply.find("```").map(|i| i + 3))?;
    let rest = &reply[open..];
    let close = rest.find("```")?;
    Some(rest[..close].trim().to_string())
}

/// What goes back to the agent when its work is sent back.
///
/// Framed as work to finish rather than as a failure. The agent is not being
/// told off; it is being told what is still missing, in the words of whoever
/// looked.
pub fn correction_prompt(reasons: &[String]) -> String {
    let mut p = String::from(
        "Your work was checked over and it is not finished yet. \
         These specific things need fixing:\n\n",
    );
    for reason in reasons {
        p.push_str(&format!("- {reason}\n"));
    }
    p.push_str(
        "\nFix them properly rather than in whatever way makes a check pass. \
         If you think one of these is wrong, say so and explain why instead of \
         changing code to satisfy it.\n",
    );
    p
}

/// Cuts a diff to [`MAX_DIFF_BYTES`], reporting whether anything was dropped.
///
/// Cuts on a line boundary so the auditor never sees half a line and reasons
/// about a fragment.
pub fn clamp_diff(diff: &str) -> (String, bool) {
    if diff.len() <= MAX_DIFF_BYTES {
        return (diff.to_string(), false);
    }
    let mut end = 0;
    for (i, _) in diff.char_indices() {
        if i > MAX_DIFF_BYTES {
            break;
        }
        end = i;
    }
    let cut = &diff[..end];
    let on_line = cut.rfind('\n').map(|i| &cut[..i]).unwrap_or(cut);
    (on_line.to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> AuditEvidence {
        AuditEvidence {
            spec: "Add a retry to the upload.".into(),
            agent_summary: "Added retries.".into(),
            diff: "--- a/x.rs\n+++ b/x.rs\n+fn retry() {}\n".into(),
            diff_truncated: false,
            changed_paths: vec!["M x.rs".into()],
            checks: vec!["cargo test: passed".into()],
        }
    }

    #[test]
    fn a_passed_verdict_is_read() {
        let v = parse_verdict("```json\n{\"verdict\":\"passed\",\"note\":\"Real retry.\"}\n```");
        assert_eq!(v, Verdict::Passed { note: "Real retry.".into() });
        assert!(!v.wants_another_pass());
    }

    #[test]
    fn a_hollow_verdict_carries_its_reasons_back() {
        let v = parse_verdict(
            "```json\n{\"verdict\":\"hollow\",\"reasons\":[\"x.rs: retry() is empty\"]}\n```",
        );
        assert_eq!(v, Verdict::Hollow { reasons: vec!["x.rs: retry() is empty".into()] });
        assert!(v.wants_another_pass());
    }

    #[test]
    fn a_rejection_with_no_reasons_is_treated_as_no_answer() {
        // Sending an agent back with nothing to fix wastes a whole pass, and
        // the run is no closer to done afterwards.
        let v = parse_verdict("```json\n{\"verdict\":\"hollow\",\"reasons\":[]}\n```");
        assert!(matches!(v, Verdict::Unavailable { .. }));
        assert!(!v.wants_another_pass());
    }

    #[test]
    fn empty_reason_strings_do_not_count_as_reasons() {
        let v = parse_verdict("```json\n{\"verdict\":\"hollow\",\"reasons\":[\"\",\"   \"]}\n```");
        assert!(matches!(v, Verdict::Unavailable { .. }));
    }

    #[test]
    fn an_unreadable_reply_never_becomes_a_rejection() {
        // The whole point: an audit that did not happen knows nothing about
        // the work. Failing closed here would send agents back over problems
        // nobody found.
        for reply in ["", "I think it looks fine!", "```json\nnot json\n```", "```\n{}\n```"] {
            let v = parse_verdict(reply);
            assert!(
                !v.wants_another_pass(),
                "{reply:?} was read as a reason to send work back"
            );
        }
    }

    #[test]
    fn a_blocked_verdict_keeps_its_reason() {
        let v = parse_verdict(
            "```json\n{\"verdict\":\"blocked\",\"blockedReason\":\"no database password\"}\n```",
        );
        assert_eq!(v, Verdict::Blocked { reason: "no database password".into() });
        assert!(!v.wants_another_pass(), "a blocked run must not be retried");
    }

    #[test]
    fn a_blocked_verdict_without_a_reason_still_parses() {
        let v = parse_verdict("```json\n{\"verdict\":\"blocked\"}\n```");
        assert!(matches!(v, Verdict::Blocked { .. }));
    }

    #[test]
    fn prose_around_the_fence_is_ignored() {
        let v = parse_verdict("Here is my review.\n\n```json\n{\"verdict\":\"passed\"}\n```\n\nHope that helps!");
        assert!(matches!(v, Verdict::Passed { .. }));
    }

    #[test]
    fn the_prompt_asks_for_the_fence_language_it_parses() {
        // The same contract `plan_proposal` guards: a prompt asking for one
        // shape and a parser reading another fails on every single run.
        let p = audit_prompt(&evidence());
        assert!(p.contains(&format!("```{FENCE_LANGUAGE}")));
    }

    #[test]
    fn the_prompt_tells_the_auditor_to_look_for_corner_cutting() {
        // This is the whole difference from a completion check. Losing these
        // instructions turns it back into "does this look done", which a
        // corner-cutting agent passes.
        let p = audit_prompt(&evidence()).to_lowercase();
        for needle in ["cannot fail", "fixed value", "name only", "real person"] {
            assert!(p.contains(needle), "the prompt stopped asking about {needle}");
        }
    }

    #[test]
    fn the_prompt_says_the_request_is_evidence_not_a_checklist() {
        let p = audit_prompt(&evidence()).to_lowercase();
        assert!(p.contains("not a checklist") || p.contains("not whether every stated item"));
    }

    #[test]
    fn the_prompt_warns_against_inventing_problems() {
        // The failure mode in the other direction: a model asked to find
        // fault will find some, and a wrong rejection costs real time.
        let p = audit_prompt(&evidence()).to_lowercase();
        assert!(p.contains("do not invent problems"));
    }

    #[test]
    fn a_run_that_changed_nothing_is_called_out_in_the_prompt() {
        let mut e = evidence();
        e.changed_paths.clear();
        let p = audit_prompt(&e);
        assert!(p.contains("changed no files"));
    }

    #[test]
    fn a_missing_spec_is_stated_rather_than_left_blank() {
        // An empty section reads as "the spec said nothing important". Saying
        // there was none is different information.
        let mut e = evidence();
        e.spec.clear();
        assert!(audit_prompt(&e).contains("Nothing written down"));
    }

    #[test]
    fn the_correction_prompt_lists_every_reason() {
        let p = correction_prompt(&["a.rs: stub".into(), "b.rs: test asserts true".into()]);
        assert!(p.contains("a.rs: stub"));
        assert!(p.contains("b.rs: test asserts true"));
        // The instruction that stops the fix being another corner cut.
        assert!(p.contains("makes a check pass"));
    }

    #[test]
    fn the_correction_prompt_lets_the_agent_push_back() {
        // An auditor can be wrong. An agent forced to comply would change
        // correct code to satisfy a bad review.
        let p = correction_prompt(&["x".into()]);
        assert!(p.contains("say so"));
    }

    #[test]
    fn a_short_diff_is_untouched() {
        let (out, cut) = clamp_diff("a\nb\n");
        assert_eq!(out, "a\nb\n");
        assert!(!cut);
    }

    #[test]
    fn a_long_diff_is_cut_on_a_line_boundary() {
        let long = "line of text\n".repeat(MAX_DIFF_BYTES / 4);
        let (out, cut) = clamp_diff(&long);
        assert!(cut);
        assert!(out.len() <= MAX_DIFF_BYTES);
        assert!(
            !out.ends_with("line of tex"),
            "cut mid-line, so the auditor sees a fragment"
        );
    }

    #[test]
    fn clamping_a_diff_of_wide_characters_does_not_panic() {
        let wide = "日本語のテキストです\n".repeat(MAX_DIFF_BYTES);
        let (_, cut) = clamp_diff(&wide);
        assert!(cut);
    }

    #[test]
    fn every_verdict_has_a_sentence_that_does_not_blame_the_user() {
        for v in [
            Verdict::Passed { note: String::new() },
            Verdict::Passed { note: "looks real".into() },
            Verdict::Hollow { reasons: vec!["one".into()] },
            Verdict::Hollow { reasons: vec!["one".into(), "two".into()] },
            Verdict::Blocked { reason: "no key".into() },
            Verdict::Unavailable { detail: "x".into() },
        ] {
            let s = v.summary();
            assert!(!s.is_empty());
            assert!(!s.to_lowercase().contains("error"), "{s}");
            assert!(!s.to_lowercase().contains("failed"), "{s}");
        }
    }

    #[test]
    fn one_problem_is_described_in_the_singular() {
        let s = Verdict::Hollow { reasons: vec!["x".into()] }.summary();
        assert!(s.contains("one thing"), "{s}");
    }
}

/// Prints the prompt an auditor would receive for a realistic corner-cut.
///
/// Ignored: it exists to be read by a person, since the thing being checked is
/// whether the wording actually provokes an adversarial read. Run with
/// `cargo test --lib audit_prompt_reads_well -- --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn audit_prompt_reads_well() {
    let evidence = AuditEvidence {
        spec: "## Retry failed uploads\n\nAn upload that fails should be retried up to three \
               times with a growing delay before it gives up."
            .into(),
        agent_summary: "Added retry support to the uploader and a test for it.".into(),
        diff: "--- a/upload.rs\n+++ b/upload.rs\n\
               @@\n\
               -pub fn upload(f: &File) -> Result<()> {\n\
               +pub fn upload(f: &File, retries: u32) -> Result<()> {\n\
                     send(f)\n\
                 }\n\
               --- a/upload_test.rs\n+++ b/upload_test.rs\n\
               @@\n\
               +#[test]\n\
               +fn retries_on_failure() {\n\
               +    assert!(true);\n\
               +}\n"
            .into(),
        diff_truncated: false,
        changed_paths: vec!["M upload.rs".into(), "A upload_test.rs".into()],
        checks: vec!["cargo test: passed".into()],
    };
    println!("{}", audit_prompt(&evidence));
}
