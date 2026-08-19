//! Drafting an edit to one spec file.
//!
//! Kept apart from `ask` deliberately. Ask's read-only promise is structural --
//! no tool loop, so no edit capability to decline -- and adding a write tool
//! there would dissolve it. This module does not write either: it returns a
//! proposed file body to the caller, who shows it as a diff. Only an explicit
//! accept, and then an explicit save, puts anything on disk.
//!
//! So there are two prompts and two commands, not one prompt with a mode flag.
//! The cost is a little duplication; the benefit is that neither path can grow
//! into the other by accident.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

/// A proposed replacement for one file of a change package.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DraftedEdit {
    /// Path relative to the change folder, echoed back so the UI cannot apply a
    /// draft to a different file than the one it asked about.
    pub file: String,
    /// The complete proposed contents of the file, not a patch.
    pub body: String,
    /// One plain-language line about what changed, for the review header.
    pub summary: String,
}

/// The system prompt for drafting.
///
/// Demands the whole file back rather than a patch. A model emitting a diff has
/// to invent line numbers and context, and a diff that almost applies is worse
/// than no diff at all -- GitWyrm computes the real difference itself from the
/// full text, which cannot drift.
pub const SYSTEM_PROMPT: &str = "\
You edit one markdown file from an OpenSpec change in a git client called GitWyrm.

You are given the file's current contents and an instruction. Return the COMPLETE
new contents of that file, with the instruction applied.

Rules:
- Return the whole file, not a patch, not a fragment, not a summary of edits.
- Change only what the instruction asks for. Leave every other line exactly as it
  is, including headings, blank lines, list markers, and trailing punctuation.
- Keep the file's existing structure and heading levels.
- Do not add commentary, apologies, or notes about what you changed.
- Write in plain language, short sentences, no jargon, no em dashes.

Reply in exactly this shape:

SUMMARY: one short sentence about what you changed
---
<the complete new file contents>

The SUMMARY line comes first, then a line containing only three hyphens, then the
file. Nothing after the file.";

/// Build the drafting prompt from the file and the user's instruction.
pub fn user_prompt(file: &str, current: &str, instruction: &str, proposal: &str) -> String {
    let mut out = String::new();
    out.push_str("The change's proposal, for context:\n\n");
    out.push_str(if proposal.trim().is_empty() {
        "(none)"
    } else {
        proposal
    });
    out.push_str("\n\n---\n\nThe file to edit: ");
    out.push_str(file);
    out.push_str("\n\nIts current contents:\n\n");
    if current.trim().is_empty() {
        out.push_str("(this file is empty or does not exist yet)");
    } else {
        out.push_str(current);
    }
    out.push_str("\n\n---\n\nThe instruction:\n\n");
    out.push_str(instruction);
    out
}

/// Parse the model's reply into a summary and the new file body.
///
/// Tolerant of a missing SUMMARY line and of the model wrapping the file in a
/// markdown code fence, both of which are common and neither of which is worth
/// failing the whole draft over. What it will not do is invent a body: an empty
/// result is an error, because silently proposing to blank someone's file is the
/// worst outcome available here.
pub fn parse_draft(reply: &str, file: &str) -> Result<DraftedEdit, AppError> {
    // Only the leading whitespace is trimmed. A trailing newline belongs to the
    // file the model is returning, and markdown files end with one -- stripping it
    // would show a spurious last-line change in the diff for every draft.
    let text = reply.trim_start();
    if text.trim().is_empty() {
        return Err(AppError::Other(
            "The AI sent back an empty reply. Try asking again.".into(),
        ));
    }

    let (summary, rest) = split_summary(text);
    let body = strip_code_fence(rest);

    if body.trim().is_empty() {
        return Err(AppError::Other(
            "The AI did not send back any file contents. Try asking again.".into(),
        ));
    }

    Ok(DraftedEdit {
        file: file.to_string(),
        body,
        summary: if summary.is_empty() {
            "Reworded this file.".to_string()
        } else {
            summary
        },
    })
}

/// Split the leading `SUMMARY:` line and the `---` separator from the body.
fn split_summary(text: &str) -> (String, &str) {
    let mut summary = String::new();
    let mut rest = text;

    if let Some(after) = strip_prefix_ignore_case(text, "summary:") {
        let (line, tail) = match after.find('\n') {
            Some(at) => (&after[..at], &after[at + 1..]),
            None => (after, ""),
        };
        summary = line.trim().to_string();
        rest = tail;
    }

    // Drop the separator line if it is there. Some models omit it when they also
    // omitted the summary, so this is independent of the branch above.
    let trimmed = rest.trim_start_matches('\n');
    if let Some(after) = trimmed.strip_prefix("---") {
        if let Some(at) = after.find('\n') {
            // Only a bare `---` line is a separator. A YAML front-matter opener or a
            // horizontal rule with text after it belongs to the file.
            if after[..at].trim().is_empty() {
                rest = &after[at + 1..];
                return (summary, rest);
            }
        }
    }
    (summary, rest)
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// Unwrap a whole-body markdown code fence, if the model added one.
///
/// Only when the fence encloses the entire body: a file that legitimately
/// *contains* fenced examples must survive untouched.
fn strip_code_fence(body: &str) -> String {
    let trimmed = body.trim();
    if !trimmed.starts_with("```") {
        // Not fenced: hand the body back untouched, trailing newline and all.
        return body.to_string();
    }
    let Some(first_newline) = trimmed.find('\n') else {
        return body.to_string();
    };
    let Some(rest) = trimmed[first_newline + 1..].strip_suffix("```") else {
        return body.to_string();
    };
    // A fence in the middle would leave more fences behind; if any remain, this
    // was not a wrapper and the original text is what the user meant.
    if rest.contains("```") {
        return body.to_string();
    }
    rest.trim_end().to_string()
}

/// Files of a change package the AI may be asked to edit.
///
/// A fixed list rather than a path check: this is what the UI offers, and it
/// keeps a drafted `file` from naming something arbitrary before it ever reaches
/// the write command's own refusal.
pub fn is_editable(file: &str) -> bool {
    matches!(file, "proposal.md" | "tasks.md" | "design.md")
        || (file.starts_with("specs/") && file.ends_with(".md") && !file.contains(".."))
}

/// Guard against a runaway instruction, mirroring `ask::check_question`.
pub const MAX_INSTRUCTION_CHARS: usize = 2_000;

pub fn check_instruction(instruction: &str) -> Result<(), AppError> {
    if instruction.trim().is_empty() {
        return Err(AppError::Other("Say what you want changed first.".into()));
    }
    if instruction.chars().count() > MAX_INSTRUCTION_CHARS {
        return Err(AppError::Other(
            "That is a lot to ask for at once. Try describing it in a few sentences.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_reply() {
        let reply = "SUMMARY: Reworded the Why section.\n---\n# Change\n\n## Why\n\nBetter.\n";
        let draft = parse_draft(reply, "proposal.md").unwrap();
        assert_eq!(draft.summary, "Reworded the Why section.");
        assert_eq!(draft.body, "# Change\n\n## Why\n\nBetter.\n");
        assert_eq!(draft.file, "proposal.md");
    }

    #[test]
    fn survives_a_missing_summary_line() {
        let draft = parse_draft("# Change\n\nJust the file.\n", "proposal.md").unwrap();
        assert!(!draft.summary.is_empty());
        assert!(draft.body.starts_with("# Change"));
    }

    #[test]
    fn unwraps_a_whole_body_code_fence() {
        let reply = "SUMMARY: x\n---\n```markdown\n# Change\n\nBody.\n```";
        let draft = parse_draft(reply, "proposal.md").unwrap();
        assert_eq!(draft.body, "# Change\n\nBody.");
    }

    #[test]
    fn keeps_fences_that_are_part_of_the_file() {
        // A spec file full of examples must not lose its fences to the unwrapper.
        let reply = "SUMMARY: x\n---\n# Doc\n\n```lua\nlocal a = 1\n```\n\nAnd more.\n";
        let draft = parse_draft(reply, "proposal.md").unwrap();
        assert!(draft.body.contains("```lua"));
        assert!(draft.body.contains("And more."));
    }

    #[test]
    fn keeps_a_horizontal_rule_that_belongs_to_the_file() {
        // `---` followed by text on the same line is not the separator.
        let reply = "SUMMARY: x\n---\n# Doc\n\n--- a rule\n\nEnd.\n";
        let draft = parse_draft(reply, "proposal.md").unwrap();
        assert!(draft.body.contains("--- a rule"));
    }

    #[test]
    fn refuses_an_empty_body() {
        assert!(parse_draft("SUMMARY: nothing\n---\n\n", "proposal.md").is_err());
        assert!(parse_draft("", "proposal.md").is_err());
    }

    #[test]
    fn editable_files_are_limited_to_the_change_package() {
        assert!(is_editable("proposal.md"));
        assert!(is_editable("tasks.md"));
        assert!(is_editable("design.md"));
        assert!(is_editable("specs/core/spec.md"));
        assert!(!is_editable("../../etc/passwd"));
        assert!(!is_editable("specs/../../escape.md"));
        assert!(!is_editable("Cargo.toml"));
    }

    #[test]
    fn an_empty_instruction_is_refused() {
        assert!(check_instruction("   ").is_err());
        assert!(check_instruction(&"x".repeat(MAX_INSTRUCTION_CHARS + 1)).is_err());
        assert!(check_instruction("Reword the Why section.").is_ok());
    }

    #[test]
    fn the_prompt_carries_the_file_and_instruction() {
        let p = user_prompt("proposal.md", "# Old\n", "Make it better.", "# Proposal\n");
        assert!(p.contains("proposal.md"));
        assert!(p.contains("# Old"));
        assert!(p.contains("Make it better."));
    }

    #[test]
    fn an_empty_file_is_described_rather_than_shown_blank() {
        let p = user_prompt("design.md", "", "Write a design.", "");
        assert!(p.contains("does not exist yet"));
    }

    #[test]
    fn the_system_prompt_demands_a_whole_file() {
        let lower = SYSTEM_PROMPT.to_ascii_lowercase();
        assert!(lower.contains("complete"));
        assert!(lower.contains("not a patch"));
    }
}
