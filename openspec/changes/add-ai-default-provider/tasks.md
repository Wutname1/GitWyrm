# Tasks

## 1. Settings shape

- [ ] 1.1 Store a selected model per provider rather than one global `ai_model`, and a
      `ai_default_provider` marker naming which configured provider is the default
- [ ] 1.2 Read the existing `ai_provider`/`ai_model` pair as the default on first load, so
      an existing install needs no reconfiguration
- [ ] 1.3 Configuring the first provider sets it as the default automatically
- [ ] 1.4 Removing the default promotes another configured provider; removing the last one
      leaves no default and every AI feature reports as not set up

## 2. Settings UI

- [ ] 2.1 List every configured provider with its own model picker, rather than one
      provider row that doubles as the app-wide choice
- [ ] 2.2 A clear way to mark one as default, and a visible indication of which is
- [ ] 2.3 Say plainly what the default governs: commit messages, commit generation, and
      Spec Desk runs

## 3. One place to read it

- [ ] 3.1 A single resolver every AI feature calls for "which provider and model", so no
      feature reads the raw setting
- [ ] 3.2 Commit-message generation and the commit-generation flow use it
- [ ] 3.3 `useSpecAi` uses it, so the Desk chip names the default

## 4. Verify

- [ ] 4.1 Configure two providers, switch the default, and confirm commit-message
      generation follows it without a restart
- [ ] 4.2 Each provider keeps its own model selection when the default changes
- [ ] 4.3 Remove the default and confirm another is promoted, with no dangling state
- [ ] 4.4 An install that had one provider configured before this change still works with
      no reconfiguration
