//! What the model is told before it starts.
//!
//! The loop that used to live here is gone: the provider CLI runs its own
//! plan/act/observe cycle, so GitWyrm hands it the whole task rather than
//! feeding it single turns. See [`crate::airun::cli_run`].

/// Whether this run has an OpenSpec task behind it, and so whether there is
/// a checkbox to tick when it finishes.
///
/// One `bool` rather than a whole context object because exactly one sentence
/// of the prompt depends on it, and the answer is already known at every call
/// site from the session's own source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskShape {
    /// An OpenSpec change or task started this run. `tasks.md` exists, and
    /// ticking the box is what ends the work.
    SpecTask,
    /// Anything else: a chat somebody typed into, an issue, a pull request, a
    /// commit, a diff, a failed check, a helper's bounded job.
    JustAsked,
}

/// What the model is told before it starts.
///
/// States the boundaries plainly rather than relying on enforcement alone.
/// What actually holds is the denied tool list the CLI is spawned with (see
/// [`super::cli_agent::denied_tools_for`]) -- a prompt is a request, not a
/// control -- but a model that knows the rules wastes fewer turns discovering
/// them by being refused.
///
/// Built rather than a constant because the constant it replaced told every
/// run it was "working inside a single git repository, on one task from a
/// spec", and that finishing meant ticking "its checkbox in the change's
/// tasks.md". For a blank chat both sentences are simply untrue: there is no
/// spec and no `tasks.md`. An agent asked "who are you?" spent its reply
/// explaining that it had been told to tick a checkbox that does not exist,
/// which is the model doing GitWyrm's honesty for it.
///
/// Two facts decide the wording, and both are known before the run starts:
/// whether a spec task is behind it, and whether it may change files at all.
/// Nothing here is guessed.
pub fn system_prompt(shape: TaskShape, can_write: bool) -> String {
    let opening = match shape {
        TaskShape::SpecTask => "You are working inside a single git repository, on one task from a spec.",
        TaskShape::JustAsked => "You are working inside a single git repository.",
    };

    let mut prompt = String::from(opening);
    prompt.push_str("\n\nRules that are enforced by the tool, not just asked of you:\n");
    prompt.push_str(
        "- You can only read and change files inside this repository. Paths outside it are refused, as is the .git folder.\n",
    );

    // Said the way it is actually enforced. A read-only run was previously
    // told it "can run commands only when this run is allowed to change
    // files" -- a rule it then had to work out applied to itself.
    if can_write {
        prompt.push_str(
            "- Every command you run is shown to the person for approval first. You can never reach the network.\n",
        );
        prompt.push_str("- You can never push. Your work stays local for the person to review.\n");
    } else {
        prompt.push_str(
            "- This run cannot change files or run commands. You can read and explain, and nothing else.\n",
        );
        prompt.push_str("- You can never reach the network, and you can never push.\n");
    }

    prompt.push_str("\nWork in small steps. Read before you change.\n");

    match shape {
        TaskShape::SpecTask => prompt.push_str(
            "When the task is done, tick its checkbox in the change's tasks.md -- that is the signal that ends the run.\n",
        ),
        // Deliberately no completion ritual. There is no checkbox, so the run
        // ends when the work does; inventing a signal here is what produced
        // the original bug in the other direction.
        TaskShape::JustAsked => prompt.push_str("Stop when the work is done, and say what you did.\n"),
    }

    prompt.push_str(
        "\nIf something is refused, do not retry it unchanged: find another way, or say plainly that you cannot.",
    );
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this function exists for.
    ///
    /// A blank chat was told it was "on one task from a spec" and that it
    /// finished by ticking "its checkbox in the change's tasks.md". Asked
    /// "who are you?", the agent replied that there was no spec and no
    /// tasks.md behind the question, so there was nothing to check off -- it
    /// spent its whole turn correcting the instructions instead of answering.
    #[test]
    fn a_chat_with_no_spec_is_never_told_about_one() {
        let prompt = system_prompt(TaskShape::JustAsked, true);
        assert!(!prompt.contains("tasks.md"), "{prompt}");
        assert!(!prompt.contains("from a spec"), "{prompt}");
        assert!(!prompt.contains("checkbox"), "{prompt}");
    }

    /// And a chat that really has one still gets the instruction, because
    /// ticking the box is what ends that run.
    #[test]
    fn a_spec_task_still_gets_its_checkbox_instruction() {
        let prompt = system_prompt(TaskShape::SpecTask, true);
        assert!(prompt.contains("tasks.md"), "{prompt}");
        assert!(prompt.contains("one task from a spec"), "{prompt}");
    }

    /// A read-only run is told it is read-only, rather than being given a
    /// conditional rule it has to work out applies to itself.
    #[test]
    fn a_read_only_run_is_told_plainly_that_it_cannot_act() {
        let prompt = system_prompt(TaskShape::JustAsked, false);
        assert!(prompt.contains("cannot change files or run commands"), "{prompt}");
        // And it is never promised an approval step for commands it may not
        // run at all.
        assert!(!prompt.contains("shown to the person for approval"), "{prompt}");
    }

    /// The enforced boundaries are stated in every shape. These are the rules
    /// the tool actually holds, and a run that does not know them wastes
    /// turns being refused.
    #[test]
    fn every_shape_states_the_boundaries_that_are_enforced() {
        for shape in [TaskShape::SpecTask, TaskShape::JustAsked] {
            for can_write in [true, false] {
                let prompt = system_prompt(shape, can_write);
                assert!(prompt.contains("inside this repository"), "{prompt}");
                assert!(prompt.contains(".git folder"), "{prompt}");
                assert!(prompt.contains("never push"), "{prompt}");
                assert!(prompt.contains("network"), "{prompt}");
            }
        }
    }
}
