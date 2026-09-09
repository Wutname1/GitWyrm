//! R6.1: a typed path from a live Plan-mode model turn to a persisted
//! [`super::graph::ProposedGraph`].
//!
//! `commands::agent_graph::agent_session_propose_graph` has always existed
//! and is correct, but nothing before this module ever called it FROM a live
//! run -- it only ever received a `ProposedGraph` a caller had already built
//! by hand (tests, or a future UI that assembles one itself). Plan mode could
//! therefore never actually produce a proposal: a Plan-mode lead ran to
//! completion exactly like Ask, and `AwaitingStartCard` (the UI for "here is
//! what I'd do, Start/Revise/Use solo") was unreachable in practice.
//!
//! ## The chosen mechanism: a fenced typed block, not a tool call
//!
//! Two options were on the table (per the reset doc's own wording):
//! "recognise a tool call by name, or parse a fenced typed block out of the
//! final message." A tool call requires the provider to expose a
//! *custom* tool this app defines and the CLI to invoke it -- `ai::agent::acp`
//! only ever sees `Incoming::ToolCall` for tools the CLI itself already
//! knows about (file edits, shell, etc; see `cli_agent::denied_tools_for` for
//! which ones a given execution refuses at launch), and there is no
//! mechanism today for GitWyrm to
//! register a NEW tool the model can call -- that would mean implementing
//! the MCP/tool-registration side of ACP, a materially larger change than
//! this gap calls for. A fenced block in the model's own final text reply
//! needs nothing from the transport layer: `SYSTEM_PROMPT` already asks the
//! model to write prose, and `cli_run::handle` already captures every
//! `TextChunk` losslessly into the transcript (`agentdesk::bridge`'s Note
//! coalescing). Parsing the transcript's own accumulated text after the
//! run ends is therefore the smaller, more honest surface: it reuses data
//! that already exists rather than adding a new transport capability.
//!
//! ## Failing visibly
//!
//! [`extract_graph_proposal`] returns a typed [`ProposalOutcome`], never a
//! silent `None`-means-solo fallback. The reset doc is explicit: "make it
//! FAIL VISIBLY when the model returns something unparseable -- never
//! silently fall back to solo without telling the user." The caller
//! (`commands::airun::route_to_agent_desk`) turns every non-`Found` variant
//! into a `Note` in the transcript explaining exactly what went wrong, so a
//! Plan-mode run that asked for a graph and got prose instead is visibly
//! different from one that got a real proposal -- never indistinguishable
//! from a solo Ask reply.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::graph::{GraphValidationError, ProposedGraph};

/// The fence language `SYSTEM_PROMPT`'s plan-mode addendum asks the model to
/// use, and the only one [`extract_graph_proposal`] recognises. Deliberately
/// specific (not bare ` ``` ` or ` ```json `) so an ordinary code sample the
/// model includes for some other reason is never mistaken for a proposal.
pub const FENCE_LANGUAGE: &str = "graph-proposal";

/// Appended to [`crate::ai::agent::run::SYSTEM_PROMPT`] for a Plan-mode lead's
/// first turn (before Start) -- tells the model the exact shape and fence
/// convention [`extract_graph_proposal`] parses back out. Kept as a function
/// of nothing (not a `const`) so a future version bump to the schema is one
/// place to change, matching `SYSTEM_PROMPT`'s own plain-string shape.
///
/// The checklist wording used to promise the app "ticks them as work lands".
/// It does not: `agentDeskPlan.ts` parses the marks out of the stored message
/// body, and nothing rewrites that body once written, so every row stayed at
/// whatever the model first typed -- and since this prompt only ever taught
/// `- [ ]`, that meant permanently pending. The parser has always understood
/// `x` and `~`; only the instruction withheld them. Teaching both, and
/// promising only what actually happens, makes the checklist truthful without
/// inventing a rewrite mechanism that does not exist.
pub fn plan_mode_instruction() -> String {
    format!(
        "You are in PLAN mode: propose a graph of work instead of doing it yet.\n\n\
Do not edit any files. Instead, reply with a short plain-language summary of your plan written as a Markdown task list, one step per line in the form `- [ ] step`, or `- [~] step` for a step already underway and `- [x] step` for one already done (the app shows these as a checklist exactly as you write them), \
then a single fenced code block written exactly as ```{FENCE_LANGUAGE} ... ``` containing ONE JSON object \
with this exact shape (a lead summary plus 0-3 helper jobs; omit helpers entirely for solo work):\n\n\
{{\n  \
  \"leadSummary\": \"one paragraph describing the overall plan\",\n  \
  \"helpers\": [\n    \
    {{\n      \
      \"nodeId\": \"short-id\",\n      \
      \"title\": \"short title\",\n      \
      \"description\": \"what this helper does\",\n      \
      \"role\": \"researcher\" | \"builder\" | \"verifier\",\n      \
      \"allowedPaths\": [\"repo-relative/glob/**\"],\n      \
      \"dependsOn\": [\"other-node-id\"],\n      \
      \"budget\": {{ \"maxTurns\": 20, \"maxSeconds\": 900 }},\n      \
      \"completion\": {{ \"kind\": \"reportsResult\" }}\n    \
    }}\n  \
  ]\n\
}}\n\n\
Set \"completion\" to what finishing actually means for that helper. \"reportsResult\" is the \
default and means the helper simply finishes. Use {{ \"kind\": \"filesChanged\", \"paths\": [\"src/parser.rs\"] }} \
when it must have changed particular files. Do NOT use \"checksPass\": GitWyrm does not watch which \
checks an agent runs, so it can never confirm one passed, and the helper would be reported unfinished \
however well it did the job. A helper that stops without meeting its condition is \
reported as unfinished and its work is NOT merged, so ask for the stricter kind only where that is \
genuinely what done means.\n\n\
Only ONE such fenced block may appear, and it must be valid JSON matching this shape exactly -- \
field names are camelCase as shown. A helper with role \"builder\", or any non-empty allowedPaths, \
needs at least one path glob. If you cannot produce a real plan, say so in plain language and do not \
include the fenced block at all."
    )
}

/// Prompt addendum for an Auto-mode lead. Unlike Plan mode, Auto has already
/// been granted permission to work: it may complete a small task itself. When
/// the task benefits from parallel or specialized work, it can instead return
/// the same typed graph block Plan mode uses. There is intentionally no
/// approval language here -- the completion path may launch a valid Auto graph
/// immediately, whereas a Plan graph waits for the user's Start decision.
pub fn auto_mode_instruction() -> String {
    format!(
        "You are the lead in AUTO mode. Work on the task directly when one agent is enough. \
If parallel or specialized help would materially improve the result, stop before doing that helper work and reply with a short plain-language explanation followed by exactly one fenced \
```{FENCE_LANGUAGE} block containing the same graph JSON shape shown below. A valid Auto graph may start immediately without a separate approval step. Do not include a graph merely to split up trivial work.\n\n\
{{\n  \
  \"leadSummary\": \"one paragraph describing the overall goal and why helpers are useful\",\n  \
  \"helpers\": [\n    \
    {{\n      \
      \"nodeId\": \"short-id\",\n      \
      \"title\": \"short title\",\n      \
      \"description\": \"what this helper does\",\n      \
      \"role\": \"researcher\" | \"builder\" | \"verifier\",\n      \
      \"allowedPaths\": [\"repo-relative/glob/**\"],\n      \
      \"dependsOn\": [\"other-node-id\"],\n      \
      \"budget\": {{ \"maxTurns\": 20, \"maxSeconds\": 900 }},\n      \
      \"completion\": {{ \"kind\": \"reportsResult\" }}\n    \
    }}\n  \
  ]\n\
}}\n\n\
Set \"completion\" to what finishing actually means for that helper. \"reportsResult\" is the \
default and means the helper simply finishes. Use {{ \"kind\": \"filesChanged\", \"paths\": [\"src/parser.rs\"] }} \
when it must have changed particular files. Do NOT use \"checksPass\": GitWyrm does not watch which \
checks an agent runs, so it can never confirm one passed, and the helper would be reported unfinished \
however well it did the job. A helper that stops without meeting its condition is \
reported as unfinished and its work is NOT merged.\n\n\
Only one graph block may appear and it must be valid JSON. A builder, or any helper with non-empty allowedPaths, needs at least one path glob. If you solve the task directly, do not include a graph block."
    )
}

/// What came back when a Plan-mode lead's final transcript text was checked
/// for a graph proposal. Every variant is something the caller renders as a
/// specific, visible outcome -- never collapsed into a bare `Option`, per
/// this module's own "fail visibly" doc comment above.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProposalOutcome {
    /// A single well-formed, schema-valid proposal was found.
    Found { graph: ProposedGraph },
    /// The reply contained no ```graph-proposal fence at all -- an ordinary
    /// Ask-shaped answer, or the model explicitly declined per the prompt's
    /// own "say so in plain language" instruction. Not necessarily wrong,
    /// but it means no graph can be started from this turn.
    NoProposalFound,
    /// More than one fence was present. Ambiguous on purpose is refused
    /// rather than guessing which one the model meant -- "only ONE such
    /// fenced block" is stated plainly in the prompt, so more than one is a
    /// model error to surface, not something to silently pick the first of.
    MultipleProposalsFound { count: usize },
    /// A fence was present but its contents are not valid JSON at all.
    MalformedJson { detail: String },
    /// The JSON parsed but does not match `ProposedGraph`'s shape (wrong
    /// field names/types, missing required fields).
    SchemaMismatch { detail: String },
    /// The JSON matched the shape but failed graph validation (a cycle, a
    /// duplicate node id, a helper with no path allowance, etc.) -- the same
    /// check `agent_session_propose_graph` itself runs, surfaced here first
    /// so the caller can report the SPECIFIC reason rather than a generic
    /// "could not start."
    Invalid { reason: GraphValidationError },
}

/// Extracts and validates the graph proposal (if any) from one Plan-mode
/// lead turn's full accumulated text.
///
/// Pure and synchronous -- no I/O, no session/store access -- so it is
/// directly unit-testable against arbitrary transcript text without a live
/// provider. The caller (`route_to_agent_desk`) is what turns a `Found`
/// result into a persisted proposal via
/// `commands::agent_graph::propose_graph_at`, and every other variant into a
/// visible transcript `Note` -- this function only decides WHAT the text
/// contains, never what happens next.
pub fn extract_graph_proposal(text: &str) -> ProposalOutcome {
    let fences = find_fences(text, FENCE_LANGUAGE);
    match fences.len() {
        0 => ProposalOutcome::NoProposalFound,
        1 => parse_one(&fences[0]),
        n => ProposalOutcome::MultipleProposalsFound { count: n },
    }
}

/// Finds every fenced block opened with ` ```{language} ` and closed with a
/// bare ` ``` ` on its own line, returning each block's inner text. A simple
/// line-oriented scan (not a full Markdown parser) is enough here: the
/// prompt asks for exactly one block in a specific, narrow shape, and this
/// only has to recognise that shape reliably, not parse arbitrary Markdown.
fn find_fences(text: &str, language: &str) -> Vec<String> {
    let open_marker = format!("```{language}");
    let mut fences = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == open_marker {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == "```" {
                    break;
                }
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(inner);
            }
            fences.push(body);
        }
    }
    fences
}

/// When this proposal was read. `now_rfc3339` is private and duplicated across
/// three command modules; rather than restructure those mid-change, the same
/// three lines live here.
fn parsed_at() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn parse_one(body: &str) -> ProposalOutcome {
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return ProposalOutcome::MalformedJson {
                detail: e.to_string(),
            };
        }
    };
    // `proposedAt` is OUR fact, not the model's: we know when we parsed this,
    // and a model asked to invent a timestamp will either omit it (failing the
    // whole proposal over bookkeeping) or guess a wrong one. Stamp it here
    // unless the reply happened to supply one, so the prompt never has to ask.
    let mut value = value;
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("proposedAt") {
            obj.insert(
                "proposedAt".to_string(),
                serde_json::Value::String(parsed_at()),
            );
        }
    }
    let graph: ProposedGraph = match serde_json::from_value(value) {
        Ok(g) => g,
        Err(e) => {
            return ProposalOutcome::SchemaMismatch {
                detail: e.to_string(),
            };
        }
    };
    if let Err(reason) = super::graph::validate_graph(&graph) {
        return ProposalOutcome::Invalid { reason };
    }
    ProposalOutcome::Found { graph }
}

#[cfg(test)]
mod tests {

    #[test]
    fn the_plan_instruction_promises_only_what_the_app_does() {
        let p = plan_mode_instruction();
        // It used to say the app "ticks them as work lands". Nothing rewrites
        // a stored message body, so that never happened -- and the prompt
        // taught only the empty box, guaranteeing every row stayed pending.
        assert!(!p.contains("ticks them"), "must not promise ticking it does not do");
        // The parser has always understood these two marks; teach them.
        assert!(p.contains("- [~] step"), "should teach the in-progress mark");
        assert!(p.contains("- [x] step"), "should teach the done mark");
    }
    use super::*;
    use crate::agentdesk::graph::{CompletionCondition, HelperRole, JobBudget};

    fn valid_json(with_helper: bool) -> String {
        if with_helper {
            r#"{
  "leadSummary": "Fix the bug in two steps",
  "helpers": [
    {
      "nodeId": "fix-core",
      "title": "Fix the core logic",
      "description": "Patch the null check",
      "role": "builder",
      "allowedPaths": ["src/**"],
      "dependsOn": [],
      "budget": { "maxTurns": 20, "maxSeconds": 900 },
      "completion": { "kind": "reportsResult" }
    }
  ]
}"#
            .to_string()
        } else {
            r#"{ "leadSummary": "Just going to look around", "helpers": [] }"#.to_string()
        }
    }

    fn fenced(body: &str) -> String {
        format!("Here is my plan.\n\n```graph-proposal\n{body}\n```\n\nLet me know if you want changes.")
    }

    #[test]
    fn no_fence_at_all_is_reported_as_no_proposal_not_an_error() {
        let outcome = extract_graph_proposal(
            "I looked at the code and here is what I found. No graph needed.",
        );
        assert_eq!(outcome, ProposalOutcome::NoProposalFound);
    }

    #[test]
    fn a_well_formed_solo_proposal_with_zero_helpers_is_found() {
        let text = fenced(&valid_json(false));
        let outcome = extract_graph_proposal(&text);
        match outcome {
            ProposalOutcome::Found { graph } => {
                assert_eq!(graph.lead_summary, "Just going to look around");
                assert!(graph.helpers.is_empty());
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn a_well_formed_proposal_with_a_helper_round_trips_every_field() {
        let text = fenced(&valid_json(true));
        let outcome = extract_graph_proposal(&text);
        match outcome {
            ProposalOutcome::Found { graph } => {
                assert_eq!(graph.helpers.len(), 1);
                let job = &graph.helpers[0];
                assert_eq!(job.node_id, "fix-core");
                assert_eq!(job.role, HelperRole::Builder);
                assert_eq!(job.allowed_paths, vec!["src/**".to_string()]);
                assert_eq!(
                    job.budget,
                    JobBudget {
                        max_turns: 20,
                        max_seconds: 900
                    }
                );
                assert_eq!(job.completion, CompletionCondition::ReportsResult);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn plain_prose_with_an_unrelated_code_fence_is_not_mistaken_for_a_proposal() {
        let text = "Here's a snippet:\n\n```rust\nfn main() {}\n```\n\nNo plan needed.";
        assert_eq!(
            extract_graph_proposal(text),
            ProposalOutcome::NoProposalFound
        );
    }

    #[test]
    fn malformed_json_inside_the_fence_fails_visibly_not_silently() {
        let text = fenced("{ not valid json at all");
        let outcome = extract_graph_proposal(&text);
        assert!(matches!(outcome, ProposalOutcome::MalformedJson { .. }));
    }

    #[test]
    fn valid_json_with_the_wrong_shape_is_a_schema_mismatch() {
        let text = fenced(r#"{ "totally": "wrong shape" }"#);
        let outcome = extract_graph_proposal(&text);
        assert!(matches!(outcome, ProposalOutcome::SchemaMismatch { .. }));
    }

    #[test]
    fn a_structurally_valid_but_semantically_invalid_graph_is_reported_as_invalid() {
        // Two helpers with the same node_id -- valid JSON, valid shape, but
        // `validate_graph` rejects duplicate ids.
        let body = r#"{
  "leadSummary": "plan",
  "helpers": [
    { "nodeId": "a", "title": "one", "description": "", "role": "researcher", "allowedPaths": [], "dependsOn": [], "budget": { "maxTurns": 5, "maxSeconds": 60 }, "completion": { "kind": "reportsResult" } },
    { "nodeId": "a", "title": "two", "description": "", "role": "researcher", "allowedPaths": [], "dependsOn": [], "budget": { "maxTurns": 5, "maxSeconds": 60 }, "completion": { "kind": "reportsResult" } }
  ]
}"#;
        let outcome = extract_graph_proposal(&fenced(body));
        assert!(matches!(
            outcome,
            ProposalOutcome::Invalid {
                reason: GraphValidationError::DuplicateNodeId { .. }
            }
        ));
    }

    #[test]
    fn more_than_one_fence_is_refused_as_ambiguous_rather_than_picking_the_first() {
        let text = format!(
            "First attempt:\n```graph-proposal\n{}\n```\nActually, here's a better one:\n```graph-proposal\n{}\n```",
            valid_json(false),
            valid_json(true)
        );
        let outcome = extract_graph_proposal(&text);
        assert_eq!(
            outcome,
            ProposalOutcome::MultipleProposalsFound { count: 2 }
        );
    }

    #[test]
    fn a_helper_missing_allowed_paths_is_invalid_via_the_same_check_propose_graph_uses() {
        let body = r#"{
  "leadSummary": "plan",
  "helpers": [
    { "nodeId": "a", "title": "one", "description": "", "role": "builder", "allowedPaths": [], "dependsOn": [], "budget": { "maxTurns": 5, "maxSeconds": 60 }, "completion": { "kind": "reportsResult" } }
  ]
}"#;
        let outcome = extract_graph_proposal(&fenced(body));
        assert!(matches!(
            outcome,
            ProposalOutcome::Invalid {
                reason: GraphValidationError::MissingAllowedPaths { .. }
            }
        ));
    }

    #[test]
    fn the_fence_marker_must_match_exactly_not_a_generic_json_fence() {
        let text = format!("```json\n{}\n```", valid_json(false));
        assert_eq!(
            extract_graph_proposal(&text),
            ProposalOutcome::NoProposalFound
        );
    }

    #[test]
    fn plan_mode_instruction_names_the_exact_fence_language_it_parses() {
        // The instruction text and the parser must never drift apart -- both
        // read from `FENCE_LANGUAGE` so this is really a guard against a
        // future hand-edit of one without the other.
        assert!(plan_mode_instruction().contains(&format!("```{FENCE_LANGUAGE}")));
    }

    /// The checklist the Plan pane renders (`agentDeskPlan.ts`) reads
    /// `- [ ] step` lines. Nothing asked the model for that shape, so the
    /// pane worked only when a model happened to write it; the instruction
    /// now asks, and this pins the exact marker the parser matches.
    /// Enforcement is worthless if nothing ever asks for a stricter kind, so
    /// the instruction offers `filesChanged` and says what an unmet condition
    /// costs.
    ///
    /// And it must NOT offer `checksPass`. Nothing in production writes a
    /// `RunStep::Check` -- a live tool call becomes `RunStep::Activity` and
    /// carries no outcome -- so `judge` can never see one pass. A lead that
    /// took the offer got its helper marked Failed and its work dropped, for
    /// work it had actually done. Offering a condition the backend cannot
    /// evaluate is the prompt promising something the code does not do.
    #[test]
    fn both_instructions_offer_only_the_completion_kinds_gitwyrm_can_judge() {
        for instruction in [plan_mode_instruction(), auto_mode_instruction()] {
            assert!(instruction.contains("filesChanged"), "{instruction}");
            // And it says what happens when one is not met, so the lead can
            // weigh asking for it.
            assert!(instruction.contains("NOT merged"), "{instruction}");
            // Named only to forbid it -- never offered as a choice.
            assert!(
                instruction.contains(r#"Do NOT use "checksPass""#),
                "{instruction}"
            );
        }
    }

    #[test]
    fn plan_mode_asks_for_the_checklist_shape_the_pane_renders() {
        assert!(plan_mode_instruction().contains("`- [ ] step`"));
    }

    #[test]
    fn auto_mode_instruction_allows_direct_work_or_one_immediate_graph() {
        let instruction = auto_mode_instruction();
        assert!(instruction.contains("Work on the task directly"));
        assert!(instruction.contains(&format!("```{FENCE_LANGUAGE}")));
        assert!(instruction.contains("start immediately"));
        assert!(!instruction.contains("Do not edit any files"));
    }
}
