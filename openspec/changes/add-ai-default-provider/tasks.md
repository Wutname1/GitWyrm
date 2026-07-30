# Tasks

## 1. Settings shape

- [x] 1.1 Store a selected model per provider rather than one global `ai_model`, and a
      `ai_default_provider` marker naming which configured provider is the default
      (`ai_models` map added. Kept `ai_provider` as the default marker instead of adding a
      new field: it has always meant "the one in use", so a new field would leave existing
      installs with no default until they reconfigured)
- [x] 1.2 Read the existing `ai_provider`/`ai_model` pair as the default on first load, so
      an existing install needs no reconfiguration (`normalizeAiModels` seeds the map from
      the legacy pair on hydrate)
- [x] 1.3 Configuring the first provider sets it as the default automatically
- [x] 1.4 Removing the default promotes another configured provider; removing the last one
      leaves no default and every AI feature reports as not set up

## 2. Settings UI

- [x] 2.1 List every configured provider with its own model picker, rather than one
      provider row that doubles as the app-wide choice
- [x] 2.2 A clear way to mark one as default, and a visible indication of which is
- [x] 2.3 Say plainly what the default governs: commit messages, commit generation, and
      Spec Desk runs

## 3. One place to read it

- [x] 3.1 A single resolver every AI feature calls for "which provider and model", so no
      feature reads the raw setting (`useAiSelection`)
- [x] 3.2 Commit-message generation and the commit-generation flow use it
- [x] 3.3 `useSpecAi` uses it, so the Desk chip names the default

## 4. Verify

- [ ] 4.1 Configure two providers, switch the default, and confirm commit-message
      generation follows it without a restart
- [ ] 4.2 Each provider keeps its own model selection when the default changes
- [ ] 4.3 Remove the default and confirm another is promoted, with no dangling state
- [ ] 4.4 An install that had one provider configured before this change still works with
      no reconfiguration

Section 4 needs a native window and two real sets of provider credentials. It is not
verifiable from a browser, where the Tauri IPC is absent. Typecheck and production build
are clean; the behaviour itself is untested.
