//! The system instruction for commit-message generation is built from two
//! parts:
//!
//! - `DEFAULT_INSTRUCTION` - the editable guidance (tone/style/what to focus
//!   on). This is what the Settings UI shows and lets users override.
//! - `FORMAT_CONTRACT` - the fixed output shape our parser depends on. It is
//!   always appended after the user's instruction and is never exposed for
//!   editing, so a custom instruction can't break parsing.

/// User-editable guidance. Kept free of any hard formatting rules.
pub const DEFAULT_INSTRUCTION: &str = "You write clear, useful git commit messages. You are given \
the staged diff and recent commit subjects for style reference.

Write a concise summary in the imperative mood, followed by a short body of 1-3 sentences \
explaining what changed and why. Match the style conventions visible in the recent subjects \
(prefixes, casing).";

/// Fixed output contract. Appended to every request; not user-editable.
const FORMAT_CONTRACT: &str = "Respond with the commit message only, in this exact shape:
- First line: the summary, no trailing period, under 72 characters.
- Then a blank line.
- Then the body. Always include a body, even for small changes.

Do not use markdown, code fences, bullet lists, or any commentary outside the commit message \
itself.";

pub fn default_instruction() -> String {
  DEFAULT_INSTRUCTION.to_string()
}

/// Combines the user's instruction (or the default when blank) with the fixed
/// format contract into the full system prompt.
pub fn build_system(user_instruction: &str) -> String {
  let instruction = user_instruction.trim();
  let instruction = if instruction.is_empty() {
    DEFAULT_INSTRUCTION
  } else {
    instruction
  };
  format!("{instruction}\n\n{FORMAT_CONTRACT}")
}
