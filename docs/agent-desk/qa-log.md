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

**Q: Why do the sync effects read `onImport` and `refetch` through refs instead of listing them as dependencies?**
A: Both change identity on nearly every render, so depending on them rebuilt the three-minute interval before it could ever fire - the toggle would have read as on and never actually checked.

**Q: Does a sync-triggered batch clear the selection the way a pressed one does?**
A: No - sync runs on a timer, and clearing would discard chats someone was midway through picking with no action of theirs to explain it.

**Q: What does a person see when some chats in a batch refuse?**
A: The tally as the headline and the first reason as the detail, since a bare "3 could not be brought in" says nothing about whether a file is damaged or the tool has gone.

| 501 | 2026-09-12 | Why was the Codex model list wrong? | It was typed into the program by hand against a version of the tool from three releases earlier, and a written-down list cannot notice when the thing it describes moves on. |
| 502 | 2026-09-12 | Can the tool be asked instead? | Yes -- it answers a request for its own models over the connection GitWyrm already opens to it, needing no project, no folder and no conversation. |
| 503 | 2026-09-12 | Was that checked against a running copy? | Yes -- it returned all six models the tool's own picker shows, including which one it would choose for itself. |
| 504 | 2026-09-12 | Is the old list gone? | No -- it is what gets shown when the tool cannot be reached, because a stale menu is still better than an empty one. |
| 505 | 2026-09-12 | How does anyone tell the two apart? | The answer now carries where it came from, so a built-in list can never quietly pass itself off as what the tool actually said. |
| 506 | 2026-09-12 | Why not just infer that from an empty list? | Because "we could not ask" and "there is nothing to offer" are different facts, and collapsing them is the mistake this project keeps finding. |
| 507 | 2026-09-12 | Does the fallback claim anything it cannot back up? | No -- it names no default and no thinking levels, because the written-down list never knew either and guessing is what made it wrong. |
| 508 | 2026-09-12 | What did this fix by accident? | How hard the tool can be asked to think, which was recorded as not possible because it is a property of each model rather than of the tool. |
| 509 | 2026-09-12 | How much does that vary? | Enough to matter -- of six models, three offer six levels, one offers five and one offers four. |
| 510 | 2026-09-12 | Is a broken or missing tool asked again next time? | Yes -- only a good answer is remembered, so someone who fixes their install is not told the old answer until they restart. |
| 511 | 2026-09-12 | What question was nobody asking? | Whether a newer version of a tool exists, as opposed to whether the installed one is too old to work at all. |
| 512 | 2026-09-12 | Why does the difference matter? | One is a refusal and the other is a note, and a tool five releases behind answers no to the first while serving badly out-of-date information. |
| 513 | 2026-09-12 | Are the two kept apart? | Yes, in the data and on screen: the refusal keeps its warning colour and the note is quiet text under a tool that works. |
| 514 | 2026-09-12 | Where does the newest version come from? | The place each tool publishes its releases, asked for one small record per tool and nothing else. |
| 515 | 2026-09-12 | Is that the same place the install command names? | For four of the five, yes -- but not for Claude, whose install line names a connector while the program GitWyrm measures is Claude Code itself. |
| 516 | 2026-09-12 | What would have happened without that distinction? | A current install would have been reported as many versions behind, because the two things number themselves completely differently. |
| 517 | 2026-09-12 | Was the network part proved or assumed? | Proved, and it failed the first time -- the request asked for a short form of the answer that this address does not serve, and was refused. |
| 518 | 2026-09-12 | Would a unit test have caught that? | No, and that is the point of keeping one check that really goes out and asks. |
| 519 | 2026-09-12 | What happens when the check cannot run? | Nothing is said about that tool, because "we could not look" must never be shown as "you are up to date". |
| 520 | 2026-09-12 | Does GitWyrm update anything itself? | No -- it shows the command to copy, for the same reason it has never run an install on anyone's behalf. |
| 521 | 2026-09-12 | What did every relaunch cost? | About four seconds of asking five tools their versions, their models and whether newer ones exist, with nothing on screen until it finished. |
| 522 | 2026-09-12 | And the next launch? | Did all of it again, having written nothing down. |
| 523 | 2026-09-12 | What happens now? | The answer from last time is shown at once, then checked again quietly, and the screen updates only if the new answer differs. |
| 524 | 2026-09-12 | What stops a remembered answer outliving the thing it describes? | It is kept only while the program is in the same place and still prints the same version, and thrown away otherwise. |
| 525 | 2026-09-12 | Why both, rather than just the version? | Because a second copy taking over can print the same version while being a different install with different models. |
| 526 | 2026-09-12 | Should a remembered list be labelled differently from one just asked for? | No, after two sub-agents argued it: the label says who wrote the list, and a remembered one is still the tool's own words, written down. |
| 527 | 2026-09-12 | What did the losing side get right? | That the check proving the tool still answers could have quietly started passing from a file without ever reaching the tool. |
| 528 | 2026-09-12 | Was that fixed? | Yes -- it now also requires something only the live conversation carries, so it cannot be satisfied by anything remembered. |
| 529 | 2026-09-12 | Can the built-in list sneak in this way? | No -- only a real answer is ever written down, so the made-up list cannot come back looking like the tool said it. |
| 530 | 2026-09-12 | What if the file is damaged or from a newer build? | It is treated as nothing remembered, which costs the time it was saving and nothing else. |
| 531 | 2026-09-12 | Why is the quiet re-check silent when nothing changed? | Because redrawing a list that already matches would move things under the pointer for no reason. |
| 532 | 2026-09-12 | How is the event name kept honest? | A test reads both sides and fails on a single wrong letter, which would otherwise look exactly like the feature working. |
| 533 | 2026-09-12 | What did running the two never-run Claude checks show? | One passed on the first try, and the other found two real faults in how GitWyrm starts a writing run. |
| 534 | 2026-09-12 | What was the first fault? | A writing run could not run a command at all, because the word GitWyrm used to ask for the tools back does not ask for them. |
| 535 | 2026-09-12 | Was the word wrong or the documentation? | The documentation says it means "all tools", and asking the program directly shows it returns twelve tools and not the one that runs commands. |
| 536 | 2026-09-12 | Why not simply name the command tool then? | Because naming any tool replaces the whole set, so naming only that one would have traded not running commands for not reading files. |
| 537 | 2026-09-12 | Can asking tools back reach past a refusal? | No, and a test now proves it: anything the run was refused is left out of the list it asks for. |
| 538 | 2026-09-12 | What was the second fault? | With commands working, one ran without anybody being asked to approve it. |
| 539 | 2026-09-12 | Is that the same as the program refusing quietly? | No, and it is worse -- a run that silently allows looks exactly like a run somebody approved. |
| 540 | 2026-09-12 | Was a fix found? | No -- three different settings were tried and none of them produced a request for approval. |
| 541 | 2026-09-12 | Why stop rather than keep guessing? | Because the message shapes were read out of an older copy of the program and are written down nowhere, so a guess would ship a safety claim resting on an assumption. |
| 542 | 2026-09-12 | What is actually at risk? | Consent, not containment -- the work still happens inside its own isolated folder, but nobody is asked first. |
| 543 | 2026-09-12 | Are read-only chats affected? | No -- those are held by refusing the tools before the program starts, rather than by asking during it. |
| 544 | 2026-09-12 | What changed in the meantime? | The comments that stated the approval step as a fact now state it as an intention, with the evidence beside them. |
| 545 | 2026-09-12 | Was the missing approval step chased further? | Yes -- the program was driven by hand outside GitWyrm to watch every message it sends during a command. |
| 546 | 2026-09-12 | Is the channel itself dead? | No -- it answers GitWyrm's opening message with its full list of commands, helpers and models, so it is listening and simply never asks. |
| 547 | 2026-09-12 | Did announcing that GitWyrm can answer approvals help? | No, in either of the two shapes tried, which brings the ruled-out list to five. |
| 548 | 2026-09-12 | Whose fault is it? | The program's -- the missing message is already reported against it, and this way of driving it is documented nowhere on purpose. |
| 549 | 2026-09-12 | Why does that matter for what we do next? | Because a guess at an undocumented message would ship a safety promise resting on an assumption, which is the thing being avoided. |
| 550 | 2026-09-12 | So what changed here instead? | GitWyrm now counts how many times it was asked, and says so in the log when a run that could change files was asked nothing. |
| 551 | 2026-09-12 | Why is being asked nothing worth saying out loud? | Because it looks exactly like a run somebody approved, and that is the only state where silence is dangerous. |
| 552 | 2026-09-12 | Does a read-only chat trip that warning? | No -- it is asked nothing by design, and the warning checks both facts before it fires. |
| 553 | 2026-09-12 | What did the last two never-run checks show? | Both passed -- a real turn against one tool, and the review step that is meant to catch work that only looks finished. |
| 554 | 2026-09-12 | Did the review step really catch anything? | Yes -- told to cut a corner, it was sent back naming both shortfalls, which is the acceptance the plan asked for and had never been run. |
| 555 | 2026-09-12 | Where should the missing-approval fact be told? | On the tool's own row where it is chosen, not on every result afterwards. |
| 556 | 2026-09-12 | Why not on each result, where the person decides to keep the work? | Because it is true of every run on that tool, so it would be a permanent label wearing warning paint, arriving after the choice it should have informed. |
| 557 | 2026-09-12 | Was that argued rather than assumed? | Yes, by two sub-agents, and the one arguing for the result panel conceded that a warning which can never be false becomes wallpaper. |
| 558 | 2026-09-12 | What did the losing side get right? | That the fact has to reach a person at all, because a line in a log is read by nobody who is choosing a tool. |
| 559 | 2026-09-12 | How is it worded? | It says the tool does not ask before running a command, and that the work still stays in a separate copy of the project. |
| 560 | 2026-09-12 | Why mention the separate copy? | Because leaving it out would imply something escaped, when what was actually lost is being asked first. |
| 561 | 2026-09-12 | Will somebody have to remember to remove it? | No -- it comes from a field on the tool's record, so it goes quiet by itself when the tool starts asking again. |
| 562 | 2026-09-12 | Is that protected? | Yes -- a test requires the note to read as a sentence, to say what the tool will not do, and to contain none of the words from the plumbing. |
| 563 | 2026-09-12 | What did re-checking the acceptance list find? | Two entries describing a state that has moved on, and one real fault they were hiding. |
| 564 | 2026-09-12 | Which entries were stale? | The one saying a sign-in blocked a whole provider, and the one saying four paths could only be reached by unit tests. |
| 565 | 2026-09-12 | Were those four paths really uncovered? | No -- all four have passing tests, so the wording read as "nobody checked" when the truth is narrower. |
| 566 | 2026-09-12 | What is the narrower truth? | The logic is tested but the wiring is not: nobody has watched a lead start a helper in the built app. |
| 567 | 2026-09-12 | Was the wiring at least confirmed to exist? | Yes -- the scheduler is called on the helper path, under a lock, re-reading after each change, which is worth checking rather than assuming. |
| 568 | 2026-09-12 | Why check that at all? | Because this project has found commands before that existed, were registered, were tested, and were never called by anything. |
| 569 | 2026-09-12 | What was the real fault? | The sentence telling somebody why a helper never started reads "a and b and c" once three things are named. |
| 570 | 2026-09-12 | Is three actually reachable? | Yes -- a lead may create three helpers and one can wait on all of them, so it is what somebody reads when their run stalls. |
| 571 | 2026-09-12 | Why does the wording matter there? | Because it is the worst possible moment for the explanation to sound machine-made. |
| 572 | 2026-09-12 | Did the tests hold up? | Yes -- put the old version back and both fail, printing the broken sentence word for word. |
| 573 | 2026-09-12 | What was checked this pass? | Whether the same kind of wording fault found last time had siblings elsewhere in what people read. |
| 574 | 2026-09-12 | Did it? | Not that kind, but a different house rule was being broken quietly in two places on screen. |
| 575 | 2026-09-12 | Which rule? | The one banning the long dash from anything a person reads, which this project writes down and had drifted from anyway. |
| 576 | 2026-09-12 | Where were they? | On the line naming a failed check, read at the moment work is accepted, and on the row telling somebody their tool is too old. |
| 577 | 2026-09-12 | Why does it keep coming back? | Because it is the punctuation a machine reaches for when joining two thoughts, so it arrives in new wording long after anyone last read the rule. |
| 578 | 2026-09-12 | Was a guard already there? | Yes, on exactly one short list, checking one field -- so it protected almost nothing. |
| 579 | 2026-09-12 | What replaced it? | A check that reads the screens themselves and fails naming the file, the line and the sentence. |
| 580 | 2026-09-12 | Does it flag comments too? | No -- the rule covers them, but a comment is read by somebody already in the file, while this text reaches people who cannot see it coming. |
| 581 | 2026-09-12 | Could the check pass by finding nothing anywhere? | No -- a second test plants one and requires it to be found, and requires a commented one to be ignored. |
| 582 | 2026-09-12 | Was it proved on a real regression? | Yes -- putting one sentence back made it fail, printing the file, the line and the sentence. |
| 583 | 2026-09-12 | Was the check added last pass actually protecting much? | No -- it named four files by hand out of twenty-six, so anything added later fell outside it silently. |
| 584 | 2026-09-12 | Did the unwatched files hide anything? | No, they were clean, but the check now finds its own files instead of trusting a list somebody has to remember. |
| 585 | 2026-09-12 | Was the wider reach proved? | Yes -- a fault planted in a file the old list never touched now fails the check by name. |
| 586 | 2026-09-12 | What did reading the approval card turn up? | That it offers six kinds of approval and a real run can only ever produce two of them. |
| 587 | 2026-09-12 | Which two? | Sending work off the machine, and everything else, which arrives unnamed but still stops and still shows what was asked. |
| 588 | 2026-09-12 | So the other four never happen? | Not today -- they are written, worded and tested, and nothing in a live run creates them. |
| 589 | 2026-09-12 | Is that a fault to fix? | Not by inventing recognisers on a guess; the honest failure is a vaguer card, never a silent action. |
| 590 | 2026-09-12 | Then what changed? | It is written down where somebody counting six kinds of approval would otherwise assume six kinds happen. |
| 591 | 2026-09-12 | What stops that note going stale? | A test that fails the moment a fifth kind becomes reachable, telling whoever did it to update the note. |
| 592 | 2026-09-12 | Was a second test written and then removed? | Yes -- it repeated one that already existed, and a check that catches nothing new reads like a guard without being one. |
