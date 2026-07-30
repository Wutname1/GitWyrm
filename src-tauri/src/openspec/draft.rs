//! Drafting a change with an AI, and drafting a single requirement to fix a
//! failed spec check.
//!
//! The cardinal rule of this module is that **nothing here writes to disk**.
//! Drafting produces a [`DraftedChange`] held in memory; the user reviews it and
//! only then does `write::create_drafted_change` put the kept parts on disk.
//! Generation and creation are deliberately separate functions with separate
//! commands so that cancelling, discarding, or crashing mid-draft cannot leave a
//! half-written change behind.
//!
//! The prompt work and the response parsing live here, apart from the Tauri
//! command, so both can be tested without a provider or a repository.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

/// One artifact the AI drafted, as offered for review.
///
/// `content` is the exact file body that would be written, so the review card
/// can preview precisely what Create produces -- a preview that paraphrased the
/// content would make "review before write" a promise the UI could not keep.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DraftedArtifact {
  /// Path relative to the change folder, e.g. `proposal.md` or
  /// `specs/openspec-core/spec.md`.
  pub path: String,
  /// Full file body.
  pub content: String,
}

/// A whole change, drafted and awaiting review. Nothing is on disk yet.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DraftedChange {
  /// The folder name the change would get. Already uniqued against existing
  /// changes, so the UI can show the user what they will actually end up with.
  pub id: String,
  /// True when `id` differs from what the user asked for because that name was
  /// taken. The UI says so rather than silently renaming.
  pub renamed: bool,
  pub proposal: DraftedArtifact,
  pub tasks: DraftedArtifact,
  /// Spec deltas. May be empty: a change that only refactors adds no
  /// requirements, and inventing one would be worse than none.
  pub deltas: Vec<DraftedArtifact>,
}

/// What the model is asked to return. Kept separate from [`DraftedChange`] so a
/// malformed response fails at parse time rather than half-populating the type
/// the UI renders.
#[derive(Debug, Deserialize)]
struct DraftResponse {
  proposal: String,
  tasks: String,
  #[serde(default)]
  deltas: Vec<DraftResponseDelta>,
}

#[derive(Debug, Deserialize)]
struct DraftResponseDelta {
  /// The capability this delta belongs to, e.g. `openspec-core`. Becomes
  /// `specs/<capability>/spec.md`.
  capability: String,
  content: String,
}

/// House style for OpenSpec artifacts, so drafts match what the CLI validates
/// and what the rest of the library already looks like.
///
/// The shape rules are not stylistic: `openspec validate` rejects a delta
/// without at least one `#### Scenario:` under each requirement, so a draft that
/// ignored them would fail the very check the user runs next.
pub const SYSTEM_PROMPT: &str = "\
You draft OpenSpec change folders for a desktop git client called GitWyrm.

Return JSON only, with this exact shape:
{\"proposal\":\"...\",\"tasks\":\"...\",\"deltas\":[{\"capability\":\"kebab-case-name\",\"content\":\"...\"}]}

proposal.md must be:
# Change: <short title>

## Why

<1-3 sentences on the problem, in plain language>

## What Changes

- <bullet per user-visible change>

## Impact

- Affected specs: <capability names, or none>
- Affected code: <paths>

tasks.md must be a `# Tasks` heading, then `## 1. <Group>` sections, then
checkbox lines like `- [ ] 1.1 Do the thing`. Number them <group>.<n>. End with
a `## <n>. Verify` group. Never mark a task done.

Each delta must be:
# <capability> Spec Delta

## ADDED Requirements

### Requirement: <name>

The system SHALL <behavior>.

#### Scenario: <name>

- WHEN <trigger>
- THEN <observable outcome>

Every requirement needs at least one scenario, or validation fails. Use
ADDED/MODIFIED/REMOVED headings only. Omit deltas entirely when the change adds
no requirements -- do not invent one.

Write for someone who does not know the codebase. Plain language, no jargon, no
em dashes, and never mention other tools or products by name.";

/// Build the user half of the drafting prompt.
///
/// Everything the AI reads is assembled here so the UI can show the user the
/// same list ("what the AI reads"): their description, the existing capability
/// names, and recent commit subjects for house style.
pub fn draft_user_prompt(description: &str, capabilities: &[String], recent_commits: &str) -> String {
  let caps = if capabilities.is_empty() {
    "(none yet)".to_string()
  } else {
    capabilities.join(", ")
  };
  format!(
    "Draft a change for this request:\n{}\n\n\
Existing capabilities in this project's specs library:\n{}\n\n\
Recent commit subjects, for house style:\n{}",
    description.trim(),
    caps,
    recent_commits.trim()
  )
}

/// Parse the model's reply into artifacts.
///
/// Models wrap JSON in prose or code fences often enough that being strict here
/// would fail drafts that are otherwise perfectly good, so the outermost JSON
/// object is extracted before parsing.
pub fn parse_draft(response: &str, id: &str, renamed: bool) -> Result<DraftedChange, AppError> {
  let json = extract_json(response)
    .ok_or_else(|| AppError::Other("The AI did not return a draft. Try again.".into()))?;
  let parsed: DraftResponse = serde_json::from_str(json).map_err(|e| {
    AppError::Other(format!("The AI's draft could not be read ({e}). Try again."))
  })?;

  if parsed.proposal.trim().is_empty() || parsed.tasks.trim().is_empty() {
    return Err(AppError::Other(
      "The AI's draft was missing a proposal or tasks. Try again.".into(),
    ));
  }

  let deltas = parsed
    .deltas
    .into_iter()
    .filter(|d| !d.content.trim().is_empty())
    .filter_map(|d| {
      // A delta whose capability name cannot be a folder is unusable, and
      // dropping it beats writing `specs//spec.md`.
      let capability = sanitize_capability(&d.capability)?;
      Some(DraftedArtifact {
        path: format!("specs/{capability}/spec.md"),
        content: ensure_trailing_newline(&d.content),
      })
    })
    .collect();

  Ok(DraftedChange {
    id: id.to_string(),
    renamed,
    proposal: DraftedArtifact {
      path: "proposal.md".to_string(),
      content: ensure_trailing_newline(&parsed.proposal),
    },
    tasks: DraftedArtifact {
      path: "tasks.md".to_string(),
      content: ensure_trailing_newline(&parsed.tasks),
    },
    deltas,
  })
}

/// Keep a capability name to what can safely be a folder: lowercase letters,
/// digits, and single hyphens.
fn sanitize_capability(raw: &str) -> Option<String> {
  let mut out = String::new();
  let mut last_dash = true; // leading dashes are dropped
  for ch in raw.trim().chars() {
    if ch.is_ascii_alphanumeric() {
      out.push(ch.to_ascii_lowercase());
      last_dash = false;
    } else if !last_dash {
      out.push('-');
      last_dash = true;
    }
  }
  let trimmed = out.trim_end_matches('-').to_string();
  (!trimmed.is_empty()).then_some(trimmed)
}

/// Files end with exactly one newline, the way every other file we write does.
fn ensure_trailing_newline(body: &str) -> String {
  let trimmed = body.trim_end();
  format!("{trimmed}\n")
}

/// The outermost `{...}` span in `text`, so fenced or chatty replies still parse.
///
/// Brace counting is string-aware: a `{` inside a JSON string value (markdown
/// bodies contain them) would otherwise unbalance the count and truncate the
/// object.
fn extract_json(text: &str) -> Option<&str> {
  let start = text.find('{')?;
  let bytes = text.as_bytes();
  let mut depth = 0usize;
  let mut in_string = false;
  let mut escaped = false;
  for (offset, &byte) in bytes.iter().enumerate().skip(start) {
    if in_string {
      if escaped {
        escaped = false;
      } else if byte == b'\\' {
        escaped = true;
      } else if byte == b'"' {
        in_string = false;
      }
      continue;
    }
    match byte {
      b'"' => in_string = true,
      b'{' => depth += 1,
      b'}' => {
        depth -= 1;
        if depth == 0 {
          return Some(&text[start..=offset]);
        }
      }
      _ => {}
    }
  }
  None
}

/// A folder name for `desired` that no existing change already uses.
///
/// Collisions get a visible `-2`, `-3`, ... suffix rather than an error: the
/// user picked a name that made sense to them, and the flow should continue with
/// something recognisable instead of sending them back to the field.
pub fn unique_change_id(desired: &str, existing: &[String]) -> (String, bool) {
  let taken = |candidate: &str| existing.iter().any(|e| e.eq_ignore_ascii_case(candidate));
  if !taken(desired) {
    return (desired.to_string(), false);
  }
  for n in 2..1000 {
    let candidate = format!("{desired}-{n}");
    if !taken(&candidate) {
      return (candidate, true);
    }
  }
  // Practically unreachable; better than looping forever.
  (format!("{desired}-new"), true)
}

#[cfg(test)]
mod tests {
  use super::*;

  const GOOD: &str = r##"{"proposal":"# Change: Add a thing\n\n## Why\n\nBecause.","tasks":"# Tasks\n\n## 1. Build\n\n- [ ] 1.1 Do it","deltas":[{"capability":"openspec-core","content":"# openspec-core Spec Delta"}]}"##;

  #[test]
  fn parses_a_well_formed_draft() {
    let draft = parse_draft(GOOD, "add-a-thing", false).expect("parses");
    assert_eq!(draft.id, "add-a-thing");
    assert!(!draft.renamed);
    assert_eq!(draft.proposal.path, "proposal.md");
    assert_eq!(draft.tasks.path, "tasks.md");
    assert_eq!(draft.deltas.len(), 1);
    assert_eq!(draft.deltas[0].path, "specs/openspec-core/spec.md");
  }

  #[test]
  fn survives_a_fenced_or_chatty_reply() {
    let wrapped = format!("Sure! Here you go:\n```json\n{GOOD}\n```\nHope that helps.");
    let draft = parse_draft(&wrapped, "add-a-thing", false).expect("parses");
    assert_eq!(draft.deltas.len(), 1);
  }

  /// Markdown bodies are full of braces. A naive brace count would stop at the
  /// first `}` inside a string and truncate the JSON.
  #[test]
  fn braces_inside_content_do_not_truncate_the_object() {
    let tricky = r##"{"proposal":"use {this} and \"that\"","tasks":"# Tasks\n\n- [ ] 1.1 x","deltas":[]}"##;
    let draft = parse_draft(tricky, "add-a-thing", false).expect("parses");
    assert!(draft.proposal.content.contains("{this}"));
    assert!(draft.proposal.content.contains(r#""that""#));
  }

  #[test]
  fn a_change_with_no_deltas_is_allowed() {
    let no_deltas = r##"{"proposal":"# Change: x\n\nWhy.","tasks":"# Tasks\n\n- [ ] 1.1 x"}"##;
    let draft = parse_draft(no_deltas, "add-x", false).expect("parses");
    assert!(draft.deltas.is_empty(), "no deltas is a valid draft, not an error");
  }

  #[test]
  fn a_missing_proposal_is_an_error_not_an_empty_file() {
    let bad = r##"{"proposal":"   ","tasks":"# Tasks"}"##;
    assert!(parse_draft(bad, "add-x", false).is_err());
  }

  #[test]
  fn junk_is_an_error_rather_than_a_panic() {
    assert!(parse_draft("I cannot help with that.", "add-x", false).is_err());
    assert!(parse_draft("", "add-x", false).is_err());
  }

  #[test]
  fn every_artifact_ends_with_exactly_one_newline() {
    let draft = parse_draft(GOOD, "add-a-thing", false).expect("parses");
    for body in [&draft.proposal.content, &draft.tasks.content] {
      assert!(body.ends_with('\n'));
      assert!(!body.ends_with("\n\n"));
    }
  }

  #[test]
  fn capability_names_become_safe_folder_names() {
    assert_eq!(sanitize_capability("openspec-core").as_deref(), Some("openspec-core"));
    assert_eq!(sanitize_capability("Spec Desk Actions").as_deref(), Some("spec-desk-actions"));
    assert_eq!(sanitize_capability("  --weird__name-- ").as_deref(), Some("weird-name"));
    assert_eq!(sanitize_capability("///"), None);
    assert_eq!(sanitize_capability(""), None);
  }

  /// A delta naming a capability that cannot be a folder is dropped rather than
  /// writing `specs//spec.md`, which would escape the change folder.
  #[test]
  fn a_delta_with_an_unusable_capability_is_dropped() {
    let bad = r##"{"proposal":"# x\n\nWhy.","tasks":"# Tasks","deltas":[{"capability":"///","content":"# x"}]}"##;
    let draft = parse_draft(bad, "add-x", false).expect("parses");
    assert!(draft.deltas.is_empty());
  }

  #[test]
  fn a_free_name_is_used_as_is() {
    let (id, renamed) = unique_change_id("add-thing", &["other".into()]);
    assert_eq!(id, "add-thing");
    assert!(!renamed);
  }

  #[test]
  fn a_taken_name_gets_a_visible_suffix() {
    let existing = vec!["add-thing".to_string(), "add-thing-2".to_string()];
    let (id, renamed) = unique_change_id("add-thing", &existing);
    assert_eq!(id, "add-thing-3");
    assert!(renamed, "the user must be told the name changed");
  }

  /// Folder names are case-insensitive on Windows, so `Add-Thing` colliding with
  /// `add-thing` has to be caught here rather than failing at create time.
  #[test]
  fn collision_check_ignores_case() {
    let (id, renamed) = unique_change_id("add-thing", &["ADD-THING".to_string()]);
    assert_eq!(id, "add-thing-2");
    assert!(renamed);
  }

  #[test]
  fn the_prompt_names_what_the_ai_reads() {
    let prompt = draft_user_prompt("do a thing", &["core".into()], "fixes: a bug");
    assert!(prompt.contains("do a thing"));
    assert!(prompt.contains("core"));
    assert!(prompt.contains("fixes: a bug"));
  }

  #[test]
  fn the_prompt_is_honest_about_an_empty_library() {
    let prompt = draft_user_prompt("do a thing", &[], "");
    assert!(prompt.contains("(none yet)"));
  }
}
