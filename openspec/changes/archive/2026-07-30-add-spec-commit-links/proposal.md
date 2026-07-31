# Change: Link changes to branches and commits in the graph

## Why

"Is this branch actually done?" should be answerable in the graph, where people already
review everything. The mockup shows spec chips on linked commits, live progress on the
branch tip, and a ✦ marker that makes AI-authored commits distinguishable forever.

## What Changes

- Branch ↔ change linking (explicit link from the Desk or branch menu; inferred from
  commit trailers otherwise)
- Commit form appends a removable `Spec: <change-id>` trailer on linked branches; the
  AI commit-message generator includes it automatically
- Spec chips on commit rows in the graph; the branch-tip chip shows `n/m` progress;
  clicking a chip opens the Spec Desk at that change
- ✦ AI marker on commits carrying an `Assisted-by:` trailer
- Graph reflects AI commits immediately (new row, branch ref moves, ahead count bumps)

## Impact

- Affected specs: `specs-graph` (new capability)
- Affected code: `src/components/domain/graph/CommitRow.tsx`, commit-form components,
  `src/lib/` trailer helpers, branch menu
- Depends on: `add-openspec-foundation`, `add-specs-sidebar-and-card` (selection),
  and `add-spec-desk-window` for chip click-through (chips can ship first and route to
  the sidebar selection until the Desk exists)
