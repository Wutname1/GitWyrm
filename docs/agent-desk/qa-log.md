# Agent Desk - decision log

One-sentence question and answer per decision. Newest section last.

## Import chats rebuild (2026-09-12)

**Q: Why not reuse `VirtualSessionList` for the chat list?**
A: It is typed to `SidebarRow`/`AgentSessionHeader` and renders GitWyrm's own chats with rename/archive/delete, so `ImportList` is a sibling using the same `@tanstack/react-virtual` approach rather than a second virtualization library.

**Q: Why did the old list need hundreds of IPC calls to render?**
A: Every row mounted three mutation hooks and a `useAgentImportContinuation` probe, so a Claude Code install with hundreds of saved chats fired one file-reading probe per row on open.

**Q: Where does the continuation probe run now?**
A: Only in the details strip for the one chat whose details are open, which is the single place it is ever needed.

**Q: Whose interaction grammar does multi-select follow?**
A: The commit list's (`views/GraphView.tsx`) - Shift ranges from the anchor, Ctrl/Cmd toggles one, plain click picks one.

**Q: Why does a plain click here select instead of deselecting the way the commit list does?**
A: These rows feed a bulk action rather than moving a cursor, so clicking a checked row and losing the whole selection would be a surprise; clearing has its own control.

**Q: What is "pick all" scoped to?**
A: Only the chats a filter is currently showing, so it can never reach past a search into chats nobody has seen - which is how a two-hundred-chat import happens by accident.

**Q: Why does the action bar separate new chats from refreshes?**
A: A button reading "Bring in 24 chats" when 19 of them are refreshes would describe work it is not doing.

**Q: How does "keep in sync" notice new chats - watch or poll?**
A: A timer every three minutes while the view is open; a client's session store is a directory-per-project tree, and arming a recursive watch over that shape is the pattern that cost this app 7.9s at repo open.

**Q: What does sync do with what it finds?**
A: Brings it in automatically (the user's choice), and reports every time it does, because silently growing the chat list is what an opt-in is meant to rule out.

**Q: Does turning sync on import the existing backlog?**
A: No - the chats present when it is switched on become the baseline, so "keep in sync" means keep up from here rather than import everything.

**Q: Is sync ever on by default?**
A: No, for any adapter, including after a damaged preferences file - reading another application's saved conversations on a timer is a choice that has to be shown to have been made.

**Q: Why does a damaged sync-preferences file read as off, when a damaged import ledger refuses instead?**
A: A lost ledger can duplicate a whole conversation, while a lost preference only stops a background check the person can see is off and turn back on.

**Q: What stops a batch importing the same chat twice?**
A: `dedupe_preserving_order` collapses repeated ids before any work, and each chat still goes through the single-chat path so the ledger check and per-message provenance dedup are the same code.

**Q: Why is there a 500-chat limit per press?**
A: A batch holds the session lock and writes a session file per chat; over the limit it refuses whole and names both numbers rather than importing an arbitrary prefix.

**Q: Was the existing `absenceClaimContract` test enough to cover the unreadable-folder rule here?**
A: No - it matches only the `query.data ?? []` shape, and reverting this surface's failure branch to "No chats found for this AI tool." left every existing check green, so a second contract now covers `isError` branches that state an absence.
