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
| 593 | 2026-09-12 | What was looked for this pass? | Logic that exists, is tested, and nothing in the app ever calls -- the fault this project has hit before. |
| 594 | 2026-09-12 | Did the first sweep find any? | It found seventeen, and the first one checked turned out to be called from a line the search itself excluded. |
| 595 | 2026-09-12 | What did the corrected sweep find? | Nine with no caller anywhere, seven of which nothing explains. |
| 596 | 2026-09-12 | Which of those matter? | The two that build a chat started from a diff or from the uncommitted changes, both named in the product's own description of how work begins. |
| 597 | 2026-09-12 | How far along are they? | The backend stores them fully and they have tests; there is simply no menu item in the app that reaches them. |
| 598 | 2026-09-12 | Is starting from a failed check in the same state? | No, worse -- the backend has the shape and there is no way to build one from the app side at all. |
| 599 | 2026-09-12 | Were those three built this pass? | No -- three entry points is a feature, not something to add on a sweep, so what changed is that they are written down instead of waiting to be rediscovered. |
| 600 | 2026-09-12 | What keeps the list honest? | A test that fails when a builder gains a caller and when one loses it, telling whoever did it which list to move it to. |
| 601 | 2026-09-12 | Did that test earn its place immediately? | Yes -- it failed on its first run and corrected two mistakes in the note being written alongside it. |
| 602 | 2026-09-12 | Which mistakes? | A builder nobody had counted, and one named in the note that does not exist at all. |
| 603 | 2026-09-12 | What was built this pass? | The missing way to start a chat from the work you have not committed yet. |
| 604 | 2026-09-12 | Why that one first? | Because everything behind it already existed and only the menu item was absent, so it is the smallest of the three gaps. |
| 605 | 2026-09-12 | Where does it live? | On the right-click menu over the uncommitted changes, next to staging and stashing, which is where somebody is already looking at that work. |
| 606 | 2026-09-12 | Does it ask the agent to change anything? | No -- it explains, because an agent that started editing work in progress unasked would be the worst possible surprise. |
| 607 | 2026-09-12 | What happens to a file that is both staged and edited again? | It is counted once, since asking about the same file twice reads as two different files. |
| 608 | 2026-09-12 | Does the item appear when there is nothing to talk about? | No, and that also keeps the builder's empty wording out of reach entirely. |
| 609 | 2026-09-12 | Did the check written last pass do its job? | Yes -- it failed the moment the wiring landed and said which list to move it to. |
| 610 | 2026-09-12 | Was that followed rather than silenced? | Yes, the list and both notes were moved together, so the record still matches what a person can actually reach. |
| 611 | 2026-09-12 | What is left of the three? | Starting from one commit's diff, which needs a menu item, and starting from a failed check, which has no way to build one yet. |
| 612 | 2026-09-12 | Was the menu seen in the running app? | No -- it needs the desktop shell to start, so the wording and the states are proved and the drawn menu is not. |
| 613 | 2026-09-12 | Should starting a chat from a commit's diff be built next? | No, decided by two sub-agents: a commit already has its own way in, and a chat about one runs against the real repository holding the real sha. |
| 614 | 2026-09-12 | What would the extra entry have added? | A list of file names the agent can already get for itself, on the longest menu in the app, with no difference a person could perceive. |
| 615 | 2026-09-12 | Would it have cost anything? | Yes -- two different stored identities for one real thing, and the weaker of the two carries no commit at all. |
| 616 | 2026-09-12 | What was that builder actually for? | A selection: the staged files, the unstaged ones, a pattern, a range -- not an object that already has its own kind. |
| 617 | 2026-09-12 | Had this been decided before? | Yes, twice in an earlier audit, which declined the same two entry points for want of a proper place to put them. |
| 618 | 2026-09-12 | Does that mean last pass ignored the precedent? | No -- the reason given then was that those rows had no right-click menu, and one has since been added, so the place it was waiting for now exists. |
| 619 | 2026-09-12 | Did the argument turn up a real fault? | Yes -- a chat about a set of files was telling the agent the first five and "and 7 more". |
| 620 | 2026-09-12 | Why did that happen? | The wording was written for a small label in the sidebar and was being reused as the instruction handed to the agent. |
| 621 | 2026-09-12 | Was the full list available? | Yes, stored in full the whole time and simply not passed on. |
| 622 | 2026-09-12 | What about the other kinds? | They keep their short wording, because each points at something the agent can go and fetch, while these two are the list. |
| 623 | 2026-09-12 | And starting from a failed check? | Further off than it looked -- nothing in the app fetches or shows whether a check passed, so it is a feature rather than a menu item. |
| 624 | 2026-09-12 | What was looked for this pass? | More of the fault found last time -- text written for a label being handed to an agent as its instructions. |
| 625 | 2026-09-12 | Was there more of it? | Yes, and worse: a chat started from an issue in the sidebar never received the issue's text at all. |
| 626 | 2026-09-12 | What did the agent get instead? | The labels and who it was assigned to, so "fix this bug" arrived as "[bug], assigned to ada". |
| 627 | 2026-09-12 | Why did nothing fail? | Because a row in a list and a fully loaded issue differ by a field that is absent rather than empty, so the check for it quietly answered no. |
| 628 | 2026-09-12 | Did the same gesture work elsewhere? | Yes -- from the issue panel it sent the real text, so the two ways of starting the same chat disagreed. |
| 629 | 2026-09-12 | Are pull requests affected too? | Yes, the same way: a review started from the sidebar described the branches and the author and never said what the change was for. |
| 630 | 2026-09-12 | What happens now? | The full issue or pull request is fetched before the chat starts, reusing whatever the panel already loaded. |
| 631 | 2026-09-12 | And if that fetch fails? | The chat still starts from the row, because starting knowing less is better than not starting. |
| 632 | 2026-09-12 | Was the builder at fault? | No -- its own tests already covered both shapes, including one named for the case with no text; the fault was which shape a screen handed it. |
| 633 | 2026-09-12 | So where is it guarded? | At the screen, where the mistake was, with the reason in the failure message. |
| 634 | 2026-09-12 | Was the missing-context fault anywhere else? | Yes -- asking an AI to explain a commit sent the first line of its message and nothing else. |
| 635 | 2026-09-12 | Why does the rest of the message matter? | Because the first line says what changed and the rest says why, and explaining a commit is a question about the why. |
| 636 | 2026-09-12 | Could the agent have looked it up itself? | No -- a chat that only explains is not allowed to run commands, so it cannot read anything it was not handed. |
| 637 | 2026-09-12 | Is that different from the diff decision last pass? | Yes, and it is the reason they came out differently: that one could read the repository for itself, and this one cannot. |
| 638 | 2026-09-12 | Where does the rest of the message go? | Under the one-line description rather than into it, because the title is a single-line chip in the sidebar. |
| 639 | 2026-09-12 | Is the message fetched on every right-click? | No -- only when the chat is actually started, so the deliberate choice to keep right-clicks cheap still holds. |
| 640 | 2026-09-12 | What if that fetch fails? | The chat starts with what the row had, which is what it always sent before. |
| 641 | 2026-09-12 | Did the first check of the new tests hold up? | No -- it reported them passing with the fix removed, because the revert itself had silently failed to match. |
| 642 | 2026-09-12 | How was that caught? | By reading the file afterwards instead of trusting the result, which showed the fix had never actually been taken out. |
| 643 | 2026-09-12 | Do they hold up now? | Yes -- with the fix genuinely removed, both fail. |
| 644 | 2026-09-13 | Does asking for a review of a pull request tell the agent what changed? | No -- it gets the description, the author and the state, and never learns which files moved. |
| 645 | 2026-09-13 | Can it find out for itself? | No -- a review is refused both commands and the network, so it can only read, and only what is already on disk. |
| 646 | 2026-09-13 | Is what is on disk even the right thing? | Not reliably -- the run uses whatever branch the person has open, which for a fork or an unfetched branch is not the change at all. |
| 647 | 2026-09-13 | Should the changed files be sent then? | Two sub-agents argued it and no: that list moves every time somebody pushes, and it would be frozen into a record the product says is never updated. |
| 648 | 2026-09-13 | Would sending the patches instead have worked? | It would have been correct and unaffordable -- thirty files of patch text already exceeds the whole budget the rest of the conversation gets. |
| 649 | 2026-09-13 | Is there a cap that would have caught that? | No, and that is the danger: the budget covers the conversation only, and source text is deliberately outside it. |
| 650 | 2026-09-13 | What was done instead? | The two ends of the change are now named, which were stored the whole time and thrown away when the prompt was built. |
| 651 | 2026-09-13 | Why is that enough to matter? | Because it turns an agent that does not know where to look into one that does, without freezing anything that goes stale. |
| 652 | 2026-09-13 | Did both sides agree on that much? | Yes -- it was the one change neither argued against, and each reached it separately. |
| 653 | 2026-09-13 | Is the bigger gap now hidden? | No -- it is written where the source is built, including why paths alone would not have helped. |
| 654 | 2026-09-13 | What was checked this pass? | The last source kind not yet audited: a conversation brought in from another tool. |
| 655 | 2026-09-13 | Is its provenance handled well? | Mostly yes, and carefully -- it records which tool it came from rather than passing as a chat started here. |
| 656 | 2026-09-13 | What was wrong? | It recorded the internal short name, so it read "imported from vscode-copilot" instead of the tool's actual name. |
| 657 | 2026-09-13 | Does that reach anybody? | Yes -- it is handed to an agent that continues the conversation, which is the one place nobody would notice it. |
| 658 | 2026-09-13 | Did the screen get it right? | Yes, and it builds the very same sentence one file away, correctly, so the two disagreed about one fact. |
| 659 | 2026-09-13 | Where does the proper name come from? | Each tool answers with its own, which is the only source that cannot drift from what the tool actually calls itself. |
| 660 | 2026-09-13 | Was a new lookup written for that? | Briefly, and then deleted -- an identical one already existed a few hundred lines below and simply was not being used here. |
| 661 | 2026-09-13 | What about a tool nobody recognises? | It falls back to the short name, matching what the screen already does rather than inventing a second behaviour. |
| 662 | 2026-09-13 | Did the test hold up? | Yes -- with the old wording put back it fails, printing both spellings side by side. |
| 663 | 2026-09-13 | What was checked once the sources were done? | The other end: what happens when the work comes back and somebody decides whether to keep it. |
| 664 | 2026-09-13 | Was the wording there wrong anywhere? | No -- every outcome is accounted for, including one that admits a commit was made while the record moved underneath it. |
| 665 | 2026-09-13 | So what was found? | The product's strongest promise, that nothing is ever pushed or posted without being asked, was written down and never checked. |
| 666 | 2026-09-13 | How was it written down? | As an instruction to grep for two names by hand, which names the check without ever running it. |
| 667 | 2026-09-13 | Was the promise actually being kept? | Yes -- both names appear only inside the comment making the promise, and nothing transmits anything. |
| 668 | 2026-09-13 | Then why change anything? | Because nothing would have failed if somebody added a push, and this is the one promise where finding out later is too late. |
| 669 | 2026-09-13 | What does the check do? | Reads the file itself and fails on a line that would send something, since what is being promised is the absence of a call. |
| 670 | 2026-09-13 | Does it ignore the comment that names those calls? | Yes, deliberately -- otherwise the explanation would have to be moved out of the file that most needs it. |
| 671 | 2026-09-13 | Did it work first time? | No -- it failed on its own search terms, so those are now assembled in pieces that cannot match the lines defining them. |
| 672 | 2026-09-13 | Was it proved on a real one? | Yes -- a push was added to the file and the check named the line and quoted it back. |
| 673 | 2026-09-13 | What was swept this pass? | The other absolute promises stated in prose, after last time found one that nothing enforced. |
| 674 | 2026-09-13 | Was the promise never to write into another tool's folder enforced? | Yes -- and better than the one fixed last time, by scanning a whole directory rather than a single file. |
| 675 | 2026-09-13 | So it was already sound? | Almost -- it skipped one file entirely, and that file was the one defining the read-only rule. |
| 676 | 2026-09-13 | Why was it skipped? | Because it names the forbidden operations in its own explanation, which would otherwise read as breaking the rule. |
| 677 | 2026-09-13 | Was that a real hole or a theoretical one? | Real -- a genuine write into somebody else's folder was added to that file and the check passed. |
| 678 | 2026-09-13 | What changed? | It now excuses comments and test fixtures rather than whole files, so every line of every file is looked at. |
| 679 | 2026-09-13 | Does it still catch the original case? | Yes -- a deletion planted in another adapter is still caught and named. |
| 680 | 2026-09-13 | Was anything else wrong there? | Yes -- the explanation described opening files a way the code does not actually use. |
| 681 | 2026-09-13 | Is the real way weaker? | No, stronger: it asks for a read handle outright, so there is no setting that could be turned the wrong way. |
| 682 | 2026-09-13 | Is the promise itself still kept? | Yes -- the only writes in that file belong to its own tests and go to temporary folders. |
| 683 | 2026-09-13 | What was checked this pass? | My own checks, after two passes running found the same blind spot in somebody else's. |
| 684 | 2026-09-13 | Did they have it? | Yes -- the punctuation check looked at two folders and six name prefixes, and everything else was invisible to it. |
| 685 | 2026-09-13 | Was anything hiding there? | Eighteen sentences people read, in tutorial steps, worktree explanations, merge banners, the onboarding tour and four dialogs. |
| 686 | 2026-09-13 | Was the list widened again? | No -- a list is what failed twice, so it now reads every screen and every module and keeps no list at all. |
| 687 | 2026-09-13 | Does that flag things that are fine? | It did once: a lone dash standing in for a missing value is a typographic mark, not a sentence joining two thoughts. |
| 688 | 2026-09-13 | How is that told apart? | By whether the dash stands alone between quotes or tags, which was checked against real examples of both kinds. |
| 689 | 2026-09-13 | Were the sentences reworded or just repunctuated? | Reworded where a colon or a full stop read better, since swapping one character for another is not the same as writing it properly. |
| 690 | 2026-09-13 | Was the wider reach proved? | Yes -- one fixed sentence was put back and the check named the file and the line it had been blind to. |
| 691 | 2026-09-13 | Is this the same fault as the last two passes? | Yes, three times now: a check that skips something to avoid matching itself goes blind exactly where it is most needed. |
| 692 | 2026-09-13 | Does anything stop a fourth? | Nothing structural -- but no check written here now carries a list of what to look at. |
