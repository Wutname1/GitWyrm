# Design

OpenSpec files remain authoritative. Agent sessions cache launch context but never own task
completion or spec text. Context records exact relative paths and task line/index so every
write uses current parser/writer conflict checks.

Plan graph nodes may reference requirement IDs/scenarios and task indices. References are
provenance, not a second dependency language. A changed task after graph draft marks the
plan stale and requires refresh before Start.

The current non-AI copy/opencode/editor actions stay available from the OpenSpec source
detail even when AI is configured.
