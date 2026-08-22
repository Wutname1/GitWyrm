# Gate 0 evidence

Recorded 2026-08-21 against commit `726a5be`, per R0.7 of
`implementation-reset-2026-08-21.md`. No product behavior is claimed here - this only
establishes that the baseline is trustworthy enough to build on.

## R0.1 / R0.2 - ownership and dirty files

`git status --short` is **empty**. The three files the reset doc listed as dirty
(`src-tauri/src/commands/agent_desk.rs`, `src/views/AgentDeskView.tsx`,
`src/components/domain/agent-desk/SessionComposer.tsx`) were reconciled and committed in
`726a5be` together with the revised task lists. Nothing was overwritten: the in-flight
transcript work from the interrupted run was verified compiling and passing before being
committed separately in `ee9a8fb`.

## R0.3 - strict OpenSpec validation

    npx openspec validate --all --strict
    Totals: 25 passed, 0 failed (25 items)

Includes `change/add-ai-agent-engine`, which the reset doc recorded as red. It validates
clean at this commit; the spec was rewritten earlier (`9c4299c`) to describe the CLI-only
architecture that actually ships.

## R0.4 - the agent-config location test

    cargo test --lib -- --test-threads=1 agent_config::locations
    test result: ok. 3 passed; 0 failed

Including `detect_clients_reports_present_only_when_a_file_actually_exists`. This test is
**flaky under parallelism, not broken**: it was observed failing during full-suite runs
and passes consistently when run serially. Client detection was NOT weakened to make it
pass - no production code was touched. The flake is test isolation (shared filesystem
state between location tests), and fixing it properly is a real task, not a Gate 0
blocker.

## R0.5 - Rust suite, one process only

    cargo test --lib -j 2 -- --test-threads=4
    test result: ok. 1014 passed; 0 failed; 1 ignored; 0 measured

Run as a single Cargo process. Note: the default parallelism links 21 integration test
binaries at once and has repeatedly been killed (exit 137) on this machine, freezing the
desktop. **Always cap jobs.**

## R0.6 - TypeScript and frontend suite

    npm run typecheck      -> exit 0, no output
    npm run test:unit      -> Test Files 54 passed (54), Tests 652 passed (652)

## Gate 0 verdict

**PASSED.** Ownership is clean, strict OpenSpec is green, TypeScript is green, frontend
tests are green, and the Rust suite is green in a single process.

Two caveats carried forward rather than hidden:

1. The `agent_config::locations` flake under parallel test threads is real and unfixed.
2. `npm run test` is not a safe command on this machine - it chains typecheck, the
   frontend suite, 21 linked Rust test binaries, and then `tauri dev`, which never exits.
   Use `npm run tauri dev` to launch, and the capped commands above to verify.

## How verification runs from here

Sub-agents do not run `cargo` or `npm`. Several concurrent builds choke the machine.
Agents write code; verification runs serially in the main session, one build at a time.
