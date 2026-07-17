//! The system instruction for commit-message generation. Users can override
//! it from Settings; this is the built-in default the "Reset" button restores.

/// Always asks for both a summary line and a body paragraph, so generated
/// commits are self-documenting.
pub const DEFAULT_INSTRUCTION: &str = "You write git commit messages. You are given the staged \
diff and recent commit subjects for style reference.

Respond with the commit message only, in this exact shape:
- First line: a concise summary in the imperative mood, no trailing period, under 72 characters.
- Then a blank line.
- Then a body of 1-3 short sentences explaining what changed and why. Always include a body, \
even for small changes.

Match the style conventions visible in the recent subjects (prefixes, casing). Do not use \
markdown, code fences, bullet lists, or any commentary outside the commit message itself.";

pub fn default_instruction() -> String {
  DEFAULT_INSTRUCTION.to_string()
}
