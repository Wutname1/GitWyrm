//! What a run may do, decided by GitWyrm rather than by the provider.
//!
//! A CLI or an API is a way to reach a model, not a trust boundary. Whatever a
//! given tool would permit on its own, these rules decide what actually
//! happens -- so they hold identically across all three transports.

use super::events::Gate;
use super::tools::{is_known_tool, PathRefusal};

/// What the engine does with a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
  /// Ordinary work: in-repo edits and the project's own checks.
  Allow,
  /// Pause and ask. Nothing else runs until the user answers.
  Ask(Gate),
  /// Not doable, and not askable. The AI is told and can adapt.
  Refuse { reason: String },
}

/// Push is refused outright -- never a gate, never an option.
///
/// A gate implies "yes" is available. It is not: a run that can push can
/// publish work the user has not seen, to a place they cannot easily take it
/// back from. Everything else a run does stays local and reviewable, and this
/// is the one rule that keeps that true.
pub const PUSH_REFUSAL: &str =
  "GitWyrm never lets a task run push. Your work stays on this machine until you \
   push it yourself.";

/// Words that mean "send this to the remote", in the forms a model produces.
const PUSH_MARKERS: &[&str] = &["git push", "git-push", "push --", "push origin", "push -u"];

/// Decides what happens for one tool call.
///
/// `path_refusal` is the result of resolving any path argument, so the caller
/// does the resolving and this stays a pure decision.
pub fn judge(tool: &str, argument: &str, path_refusal: Option<PathRefusal>) -> Verdict {
  // Push first, before anything else can make it look like an ordinary edit.
  if mentions_push(tool) || mentions_push(argument) {
    return Verdict::Refuse { reason: PUSH_REFUSAL.into() };
  }

  if let Some(refusal) = path_refusal {
    return match refusal {
      // Outside the repository is a gate, not a flat refusal: there are
      // legitimate reasons, and the user is the one who should decide.
      PathRefusal::OutsideRepo | PathRefusal::NotRelative => Verdict::Ask(Gate::OutsideRepo {
        path: argument.to_string(),
        summary: format!(
          "This wants to touch {argument}, which is outside the folder you opened."
        ),
      }),
      // Git's own store is never a file edit.
      PathRefusal::GitInternals => Verdict::Refuse {
        reason: "GitWyrm doesn't let a task run edit git's own files. That would rewrite \
                 history rather than change your code."
          .into(),
      },
    };
  }

  if !is_known_tool(tool) {
    return Verdict::Ask(Gate::UnknownTool {
      name: tool.to_string(),
      summary: format!("This wants to run {tool}, which isn't one of the things GitWyrm \
                        normally lets a task do."),
    });
  }

  Verdict::Allow
}

fn mentions_push(text: &str) -> bool {
  let lower = text.to_lowercase();
  PUSH_MARKERS.iter().any(|m| lower.contains(m))
}

/// Whether a branch is safe to run against.
///
/// A run edits files in place. Doing that with a different branch checked out
/// than the one the task is linked to means the work lands somewhere the user
/// was not looking.
pub fn branch_is_runnable(linked: Option<&str>, current: &str) -> Result<(), String> {
  match linked {
    None => Ok(()),
    Some(l) if l == current => Ok(()),
    Some(l) => Err(format!(
      "This task is linked to the branch \"{l}\", but you're on \"{current}\". \
       Switch to \"{l}\" first so the work lands where you expect."
    )),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn push_is_refused_not_gated() {
    // The central rule. A gate would imply "yes" is on offer; it is not.
    for text in [
      "git push",
      "git push origin main",
      "GIT PUSH --force",
      "git-push",
      "run: git push -u origin HEAD",
    ] {
      match judge("run_check", text, None) {
        Verdict::Refuse { reason } => assert!(reason.contains("never")),
        other => panic!("{text} should be refused, got {other:?}"),
      }
    }
  }

  #[test]
  fn push_named_as_the_tool_is_also_refused() {
    assert!(matches!(judge("git push", "", None), Verdict::Refuse { .. }));
  }

  #[test]
  fn ordinary_edits_are_allowed_without_a_gate() {
    // In-repo edits and the project's checks are the job, not a side effect.
    assert_eq!(judge("edit_file", "src/main.rs", None), Verdict::Allow);
    assert_eq!(judge("read_file", "src/main.rs", None), Verdict::Allow);
    assert_eq!(judge("run_check", "typecheck", None), Verdict::Allow);
    assert_eq!(judge("list_directory", "src", None), Verdict::Allow);
  }

  #[test]
  fn outside_the_repo_asks_rather_than_refusing() {
    match judge("read_file", "/etc/hosts", Some(PathRefusal::NotRelative)) {
      Verdict::Ask(Gate::OutsideRepo { summary, .. }) => {
        assert!(summary.contains("outside"), "{summary}");
      }
      other => panic!("expected a gate, got {other:?}"),
    }
  }

  #[test]
  fn git_internals_are_refused_outright() {
    assert!(matches!(
      judge("edit_file", ".git/config", Some(PathRefusal::GitInternals)),
      Verdict::Refuse { .. }
    ));
  }

  #[test]
  fn an_unknown_tool_is_gated_never_executed() {
    match judge("bash", "ls", None) {
      Verdict::Ask(Gate::UnknownTool { name, .. }) => assert_eq!(name, "bash"),
      other => panic!("expected a gate, got {other:?}"),
    }
  }

  #[test]
  fn push_beats_every_other_rule() {
    // Even dressed as an ordinary in-repo edit, a push is still a push.
    assert!(matches!(judge("edit_file", "git push origin", None), Verdict::Refuse { .. }));
  }

  #[test]
  fn a_run_refuses_a_different_branch() {
    assert!(branch_is_runnable(Some("feature"), "feature").is_ok());
    assert!(branch_is_runnable(None, "anything").is_ok());
    let err = branch_is_runnable(Some("feature"), "main").unwrap_err();
    assert!(err.contains("feature") && err.contains("main"), "{err}");
  }

  #[test]
  fn no_refusal_or_gate_uses_jargon() {
    let cases = [
      judge("edit_file", "git push", None),
      judge("edit_file", ".git/config", Some(PathRefusal::GitInternals)),
      judge("bash", "ls", None),
      judge("read_file", "/etc", Some(PathRefusal::OutsideRepo)),
    ];
    for v in &cases {
      let text = match v {
        Verdict::Refuse { reason } => reason.clone(),
        Verdict::Ask(g) => g.summary().to_string(),
        Verdict::Allow => continue,
      };
      for jargon in ["subprocess", "tainted", "ACP", "stdout", "JSON-RPC"] {
        assert!(!text.contains(jargon), "{text} contains {jargon}");
      }
    }
  }
}
