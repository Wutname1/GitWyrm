//! Asking a model to resolve one conflicted file.
//!
//! The output is a *draft*. It is loaded into the editor for the user to read
//! and accept, never written to disk or staged on the model's say-so -- the
//! same stance the spec drafting commands take. A merge resolution is a claim
//! about intent that only the author can confirm, and a wrong one is silent:
//! the file compiles, the tests pass, and a behaviour someone deliberately
//! added is gone.

use crate::error::AppError;

/// What the model is asked to do, and the shape the answer must arrive in.
///
/// The output contract is strict because the reply is written straight into an
/// editor buffer: a chat-style preamble or a markdown fence would land in the
/// user's source file as text. Asked for the whole file rather than a patch for
/// the same reason -- a patch would need applying, and a mis-applied patch is a
/// worse failure than a visibly wrong file.
pub const SYSTEM_PROMPT: &str = "\
You resolve merge conflicts in source files.

You are given one file that git could not merge. It contains conflict markers:

<<<<<<< marks the start of our version (the branch being merged into)
||||||| marks the common ancestor, when present
======= separates the two versions
>>>>>>> marks the end of their version (the incoming branch)

Return the complete resolved file and nothing else.

Rules:
- Output the entire file, from its first line to its last.
- Remove every conflict marker line.
- Keep the intent of BOTH sides wherever they changed different things. Most
  conflicts are two unrelated edits that happen to sit close together, and the
  correct resolution keeps both.
- Choose one side only when the two genuinely contradict each other.
- Never invent code that was in neither side, and never fix unrelated problems
  you notice. Resolving the conflict is the whole task.
- Preserve the file's existing indentation style and line endings.
- Do not wrap the output in a markdown code fence.
- Do not explain what you did.";

/// The conflicted file, with its path for language and convention cues.
pub fn user_prompt(path: &str, conflict_text: &str) -> String {
    format!("File: {path}\n\nConflicted content:\n{conflict_text}")
}

/// A model reply that is too large to be one file is a runaway, not an answer.
const MAX_OUTPUT_TOKENS: u32 = 8_192;

/// One file is a small ask; a reply that has not arrived in two minutes is not
/// coming. Shorter than the shared five-minute default, which is sized for
/// multi-file drafting.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Strip a markdown fence the model added despite being asked not to.
///
/// Worth handling rather than rejecting: a fenced answer is otherwise correct,
/// and refusing it would send the user back to a spinner for a formatting slip.
/// Only unwrapped when the fence encloses the whole reply -- a fence in the
/// middle is part of the file (a markdown document, say) and must survive.
fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let Some(after_open) = trimmed.find('\n') else {
        return text;
    };
    let Some(close) = trimmed.rfind("```") else {
        return text;
    };
    if close <= after_open {
        return text;
    }
    // Everything after the opening fence's language tag, up to the last fence.
    trimmed[after_open + 1..close].trim_end_matches('\n')
}

/// Ask `provider`/`model` to resolve `conflict_text`, returning the file body.
///
/// Errors rather than returning text that still holds markers: handing back a
/// still-conflicted file would look like success and quietly invite the user to
/// stage it.
pub async fn resolve(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
    path: &str,
    conflict_text: &str,
) -> Result<String, AppError> {
    let user = user_prompt(path, conflict_text);
    let reply = crate::ai::complete::complete_with(
        app,
        provider,
        model,
        SYSTEM_PROMPT,
        &user,
        MAX_OUTPUT_TOKENS,
        TIMEOUT,
    )
    .await?;

    let resolved = strip_code_fence(&reply);

    if resolved.trim().is_empty() {
        return Err(AppError::Other(
            "The AI returned an empty file. Nothing was changed.".into(),
        ));
    }

    if has_conflict_markers(resolved) {
        return Err(AppError::Other(
            "The AI left conflict markers in the file, so its answer was discarded. \
             Try again, or resolve this one by hand."
                .into(),
        ));
    }

    Ok(resolved.to_string())
}

/// True when any line still opens, splits, or closes a conflict.
///
/// Matches on the line prefix, with the trailing `\r` of a CRLF file stripped
/// first -- a marker in a CRLF file is `=======\r`, which no equality test
/// against `"======="` would ever catch.
fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|raw| {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        line.starts_with("<<<<<<<")
            || line.starts_with("=======")
            || line.starts_with(">>>>>>>")
            || line.starts_with("|||||||")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markers_in_lf_and_crlf_text() {
        assert!(has_conflict_markers("a\n<<<<<<< HEAD\nb\n"));
        assert!(has_conflict_markers("a\r\n=======\r\nb\r\n"));
        assert!(has_conflict_markers("a\n>>>>>>> other\n"));
        assert!(has_conflict_markers("a\n||||||| 1234567\n"));
    }

    #[test]
    fn accepts_a_clean_file() {
        assert!(!has_conflict_markers("fn main() {\n    println!(\"hi\");\n}\n"));
    }

    #[test]
    fn unwraps_a_fenced_reply() {
        assert_eq!(strip_code_fence("```rust\nfn a() {}\n```"), "fn a() {}");
        assert_eq!(strip_code_fence("```\nplain\n```"), "plain");
    }

    #[test]
    fn leaves_unfenced_text_alone() {
        assert_eq!(strip_code_fence("fn a() {}\n"), "fn a() {}\n");
    }

    /// A markdown file legitimately contains fences; only an enclosing one goes.
    #[test]
    fn keeps_fences_that_are_part_of_the_file() {
        let doc = "# Title\n\n```sh\nls\n```\n\nmore text\n";
        assert_eq!(strip_code_fence(doc), doc);
    }
}
