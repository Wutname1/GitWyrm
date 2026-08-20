//! Skill/MCP configuration discovery and safe cross-client sync.
//!
//! See `openspec/changes/agent-desk-configuration-sync/` for the contract
//! this module implements and `docs/agent-desk/architecture.md` section 13
//! for the safety shape (discovery/write separation, hash-gated apply,
//! backup+receipt, hash-gated undo).
//!
//! Kept as its own top-level module (not nested under `agentdesk`) and using
//! its own minimal client-location discovery in [`locations`] rather than
//! the external-chat-import adapter trait, since that adapter module was
//! being built concurrently in a separate session when this was written --
//! see the note at the top of `locations.rs` for the follow-up to unify them.

pub mod json_patch;
pub mod locations;
pub mod model;
pub mod normalize;
pub mod plan;
pub mod readers;
pub mod redact;
pub mod writers;
