//! Answering questions about a change, without the ability to change anything.
//!
//! Read-only **by construction**, which is the whole point: this goes through
//! `ai::complete`, a one-shot prompt/response call with no tool loop attached.
//! There is no edit tool to decline because there is no tool surface at all. A
//! session that merely *refused* to write would still be one prompt-injection
//! away from writing.
//!
//! The other half of the contract is that the user can always tell which mode
//! they are in. This module supplies the pieces the UI needs for that: answers
//! carry the sources they were grounded in, and a request to *do* work comes
//! back flagged as an escalation rather than being quietly attempted.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppError;

/// A document the answer drew on, for the citation chips.
///
/// `tab` is the Desk tab a chip opens. Kept as a plain string matching the tab
/// keys the frontend already uses, so a new tab does not need a change here.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AskSource {
  /// What to show on the chip, e.g. "proposal.md" or "specs/ai-ask/spec.md".
  pub label: String,
  /// Desk tab to open: "proposal", "tasks", "deltas", or "design".
  pub tab: String,
}

/// One answer from an ask session.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AskAnswer {
  pub text: String,
  pub sources: Vec<AskSource>,
  /// True when the user asked for work to be done rather than explained. The UI
  /// turns this into a "Start a run with AI" button; promotion from read-only to
  /// write is always one visible click.
  pub wants_run: bool,
}

/// The system prompt. Says plainly what this session can and cannot do, because
/// a model told it can edit will happily claim it did.
pub const SYSTEM_PROMPT: &str = "\
You answer questions about one OpenSpec change in a git client called GitWyrm.

You are READ-ONLY. You cannot edit files, tick tasks, run commands, or touch git.
Never claim to have made a change, and never write out a patch as though applying
it. If the user asks you to DO something rather than explain something, say that
you cannot make changes here, and tell them a monitored run can -- then stop. Do
not attempt the work anyway.

Answer from the documents you are given. When you do not know, say so plainly
rather than guessing; a confident wrong answer about someone's own spec is worse
than none.

Write for someone who does not know this codebase. Plain language, short
sentences, no jargon, no em dashes.

End every answer with a Sources line naming the documents you used, exactly:
Sources: proposal.md, tasks.md
Name only documents you were actually given. If none applied, write:
Sources: none";

/// Build the question half of the prompt from the change's own documents.
///
/// Everything the model sees is assembled here, so the UI can honestly tell the
/// user what was read.
pub fn user_prompt(
  change_id: &str,
  proposal: &str,
  tasks: &str,
  design: Option<&str>,
  deltas: &[(String, String)],
  question: &str,
) -> String {
  let mut out = format!("Change id: {change_id}\n\n--- proposal.md ---\n{}\n", proposal.trim());
  out.push_str(&format!("\n--- tasks.md ---\n{}\n", tasks.trim()));
  if let Some(design) = design {
    out.push_str(&format!("\n--- design.md ---\n{}\n", design.trim()));
  }
  for (path, body) in deltas {
    out.push_str(&format!("\n--- {path} ---\n{}\n", body.trim()));
  }
  out.push_str(&format!("\n--- question ---\n{}", question.trim()));
  out
}

/// Phrases that mean "do the work", not "explain the work".
///
/// Deliberately narrow. A false positive turns a real question into an
/// escalation prompt, which is annoying but harmless; the model is separately
/// instructed to refuse, so this is a UI affordance rather than the guard
/// itself. The actual guarantee is that no mutation tool exists.
const RUN_INTENTS: [&str; 12] = [
  "just do it",
  "do it for me",
  "do task",
  "implement task",
  "implement it",
  "write the code",
  "make the change",
  "make the changes",
  "fix it for me",
  "go ahead and",
  "can you do it",
  "build it for me",
];

/// Whether `question` reads as a request to perform work.
pub fn wants_run(question: &str) -> bool {
  let q = question.to_ascii_lowercase();
  RUN_INTENTS.iter().any(|intent| q.contains(intent))
}

/// Split a model reply into its prose and the sources it named.
///
/// The trailing `Sources:` line is removed from the text: it is rendered as
/// chips, and leaving it in the bubble too would say everything twice.
pub fn parse_answer(reply: &str, available: &[AskSource], asked_for_work: bool) -> AskAnswer {
  let trimmed = reply.trim();
  let (body, named) = match find_sources_line(trimmed) {
    Some((at, line)) => (trimmed[..at].trim_end().to_string(), line),
    None => (trimmed.to_string(), String::new()),
  };

  // Match what the model named against the documents it was actually given, so
  // a hallucinated filename cannot become a chip that opens nothing.
  let lower = named.to_ascii_lowercase();
  let sources = available
    .iter()
    .filter(|s| lower.contains(&s.label.to_ascii_lowercase()))
    .cloned()
    .collect();

  AskAnswer {
    text: if body.is_empty() { trimmed.to_string() } else { body },
    sources,
    wants_run: asked_for_work,
  }
}

/// Byte offset and content of the final `Sources:` line, if there is one.
fn find_sources_line(text: &str) -> Option<(usize, String)> {
  let mut offset = 0usize;
  let mut found: Option<(usize, String)> = None;
  for line in text.split_inclusive('\n') {
    let trimmed = line.trim_start();
    if trimmed.len() >= 8 && trimmed[..8].eq_ignore_ascii_case("sources:") {
      found = Some((offset, trimmed.to_string()));
    }
    offset += line.len();
  }
  found
}

/// Guard against a runaway question. Long enough for a real paragraph, short
/// enough that a pasted file cannot blow up the prompt.
pub const MAX_QUESTION_CHARS: usize = 2_000;

/// Reject an empty or oversized question before spending a token on it.
pub fn check_question(question: &str) -> Result<(), AppError> {
  if question.trim().is_empty() {
    return Err(AppError::Other("Type a question first.".into()));
  }
  if question.chars().count() > MAX_QUESTION_CHARS {
    return Err(AppError::Other(
      "That question is too long. Try asking it in a few sentences.".into(),
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sources() -> Vec<AskSource> {
    vec![
      AskSource { label: "proposal.md".into(), tab: "proposal".into() },
      AskSource { label: "tasks.md".into(), tab: "tasks".into() },
      AskSource { label: "specs/ai-ask/spec.md".into(), tab: "deltas".into() },
    ]
  }

  #[test]
  fn splits_the_sources_line_off_the_answer() {
    let reply = "It works that way because of the review step.\n\nSources: proposal.md, tasks.md";
    let answer = parse_answer(reply, &sources(), false);
    assert_eq!(answer.text, "It works that way because of the review step.");
    assert!(!answer.text.contains("Sources:"), "the line is rendered as chips, not twice");
    assert_eq!(answer.sources.len(), 2);
  }

  /// A model naming a file it was never given must not produce a chip that
  /// opens an empty tab.
  #[test]
  fn a_hallucinated_source_is_dropped() {
    let reply = "Because of reasons.\n\nSources: proposal.md, imaginary.md";
    let answer = parse_answer(reply, &sources(), false);
    let labels: Vec<&str> = answer.sources.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, vec!["proposal.md"]);
  }

  #[test]
  fn sources_none_yields_no_chips() {
    let answer = parse_answer("I do not know.\n\nSources: none", &sources(), false);
    assert!(answer.sources.is_empty());
    assert_eq!(answer.text, "I do not know.");
  }

  #[test]
  fn an_answer_without_a_sources_line_still_reads() {
    let answer = parse_answer("Just prose, no citations.", &sources(), false);
    assert_eq!(answer.text, "Just prose, no citations.");
    assert!(answer.sources.is_empty());
  }

  /// The word "sources" inside the prose must not be mistaken for the trailer.
  #[test]
  fn prose_mentioning_sources_is_not_truncated() {
    let reply = "The sources: they are listed in the proposal.\n\nSources: proposal.md";
    let answer = parse_answer(reply, &sources(), false);
    assert!(answer.text.contains("The sources: they are listed"));
    assert_eq!(answer.sources.len(), 1);
  }

  #[test]
  fn deltas_map_to_the_deltas_tab() {
    let reply = "See the delta.\n\nSources: specs/ai-ask/spec.md";
    let answer = parse_answer(reply, &sources(), false);
    assert_eq!(answer.sources.len(), 1);
    assert_eq!(answer.sources[0].tab, "deltas");
  }

  #[test]
  fn do_it_for_me_reads_as_an_escalation() {
    for q in [
      "just do it",
      "Just do task 3 for me",
      "can you do it please",
      "go ahead and implement it",
      "Write the code for this",
    ] {
      assert!(wants_run(q), "{q:?} should offer a run");
    }
  }

  #[test]
  fn a_real_question_is_not_an_escalation() {
    for q in [
      "why is it built this way?",
      "what does task 3 mean?",
      "which file does this touch?",
      "how do runs differ from ask?",
    ] {
      assert!(!wants_run(q), "{q:?} should stay a question");
    }
  }

  #[test]
  fn the_prompt_carries_every_document_it_was_given() {
    let deltas = vec![("specs/ai-ask/spec.md".to_string(), "# delta body".to_string())];
    let prompt = user_prompt(
      "add-ai-ask-mode",
      "# proposal body",
      "# tasks body",
      Some("# design body"),
      &deltas,
      "why?",
    );
    for expected in [
      "add-ai-ask-mode",
      "# proposal body",
      "# tasks body",
      "# design body",
      "specs/ai-ask/spec.md",
      "why?",
    ] {
      assert!(prompt.contains(expected), "prompt should carry {expected:?}");
    }
  }

  #[test]
  fn a_change_without_a_design_doc_omits_it_rather_than_faking_one() {
    let prompt = user_prompt("x", "p", "t", None, &[], "q");
    assert!(!prompt.contains("design.md"));
  }

  #[test]
  fn the_system_prompt_states_the_read_only_rule() {
    // Guards the copy against an edit that would let the model claim it wrote.
    assert!(SYSTEM_PROMPT.contains("READ-ONLY"));
    assert!(SYSTEM_PROMPT.to_ascii_lowercase().contains("cannot edit files"));
  }

  #[test]
  fn empty_and_oversized_questions_are_refused() {
    assert!(check_question("   ").is_err());
    assert!(check_question(&"a".repeat(MAX_QUESTION_CHARS + 1)).is_err());
    assert!(check_question("why?").is_ok());
  }
}
