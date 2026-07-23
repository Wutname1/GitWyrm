# GitWyrm design-sync notes

Repo-specific gotchas for future syncs. Read before re-syncing.

## Source shape
- **This is an app, not a component library.** No `main`/`module`/`exports` in package.json, no `dist/` library entry. Package name is `gitwyrm-mockup`.
- Components live under `src/components/` — `ui/` (shadcn "new-york" primitives) + `domain/` + `modals/`. No barrels.
- **Synthetic barrel entry**: `.ds-sync/gen-barrel.mjs` writes `src/components/_ds_barrel.ts` re-exporting every component file. The build is run with `--entry ./src/components/_ds_barrel.ts` so PKG_DIR walks up to the repo-root package.json (name gives version + src/ + css). Without `--entry` the converter looks for `node_modules/gitwyrm-mockup` (doesn't exist) and fails.
- **componentSrcMap is fully enumerated** (154 entries) by `.ds-sync/gen-srcmap.mjs`. In synth mode with no `.d.ts` dist, `exportedNames` returns empty and discovery would yield 0 without an explicit map. Regenerate with gen-srcmap if components are added/removed.
- 154 exports = ~99 files; the extra are shadcn compound sub-parts (DialogContent, ContextMenuItem, DropdownMenuTrigger, ...). All ship in the bundle as `window.GitWyrm.*`. Sub-parts get floor cards by design.

## CSS / Tailwind v4
- `src/index.css` is Tailwind v4 SOURCE (`@import "tailwindcss"` etc.) — NOT shippable raw; the converter can't resolve those imports (`[CSS_IMPORT_MISSING]`).
- Ship the **Vite-COMPILED** css instead: `dist/assets/index-<hash>.css` (expanded utilities, resolved `--gw-*` tokens, 15 real `@font-face`).
- `.ds-sync/stage-css.mjs` copies the hashed compiled css to a stable `dist/gitwyrm.css` and rewrites root-absolute `url(/assets/...)` -> `url(./assets/...)` so extractFonts resolves fonts relative to dist/. `cfg.cssEntry` points at `dist/gitwyrm.css`.
- **buildCmd** = `npm run build && node .ds-sync/gen-barrel.mjs && node .ds-sync/stage-css.mjs` — re-run before every build so the barrel + staged css are fresh.

## Render check / browser
- No Playwright cache on this machine. Using system Chrome via `DS_CHROMIUM_PATH="/c/Program Files/Google/Chrome/Application/chrome.exe"` + the small `playwright` npm package installed in `.ds-sync/` (no 200MB chromium download). Export DS_CHROMIUM_PATH before running package-validate / package-capture.

## Tauri coupling (why most domain components are floor cards)
- 56 of 66 domain components call `@tauri-apps` IPC, Zustand `/stores/`, `/lib/bindings`, or react-query. They render blank in a browser without mocking the Tauri runtime + providers.
- Authored-preview scope (user chose 2026-07-22): the 18 pure `ui/*` primitives + ~10 pure domain components (Avatar, HunkBar, GithubIcon, ChangeSizeIndicator, etc.). Everything else ships functional + floor card, authorable on a later re-sync.

## Known render warns (triaged — not new issues)
- `[FONT_MISSING] "Inter"`: the app CSS has an odd mono stack `"Inter", ui-monospace, "SF Mono", monospace` (looks like a source quirk — Inter used as mono). Bare "Inter" has no @font-face and falls back to ui-monospace by the stack's own design. The REAL families (Inter Variable, Sora, Geist Mono, IBM Plex Sans Variable) all ship @font-face + font files. Not a real gap.

## Dark preview surface (GwThemeSurface provider)
- The design-sync card canvas defaults to white; GitWyrm is dark-only. `.design-sync/ds-theme-surface.tsx` exports `GwThemeSurface`, wired as `cfg.provider.component` (with `TooltipProvider` as `inner`). It paints `--gw-bg` + `--gw-text` behind every card. Bundled via `cfg.extraEntries`. Without it, inputs/muted text/ghost buttons wash out.
- **DO NOT add QueryClientProvider (or any `@tanstack/react-query` import) to GwThemeSurface.** It hangs the ts-morph `.d.ts` pass indefinitely (react-query's type graph is enormous — the build stalls at "parsed 1 .d.ts files" and never reaches `[DTS] N/154`). Confirmed by isolation test 2026-07-22. Components needing a QueryClient (BranchSidebarItem and other `useRefDnd`/`useQuery` users) are left as floor cards instead.

## Authored previews (40 components graded good)
- Solo: Button, Input, Textarea, Dialog. Overlays (batch A): ContextMenu, DropdownMenu, Popover, HoverCard, Collapsible, Tooltip. Controls (batch B): Slider, Separator, ScrollArea, FormDialog, PendingIndicator, PendingMenuItem, ResizeHandle, TooltipButton. Domain (batch C): Avatar, HunkBar, GithubIcon, ChangeSizeIndicator, LineSelectionBar, DiffLineMenu, SectionItemRow, StatusBadge.
- **Overlay open-state pattern**: Radix overlays render in a portal, closed by default. To show them open in a card: compose the full parent with `defaultOpen` (or controlled `open` for HoverCard/ContextMenu — they aren't defaultOpen-able) + `modal={false}` + `onEscapeKeyDown`/`onPointerDownOutside` preventDefault on Content. Then set `cfg.overrides.<Name>: {cardMode:"single", primaryStory, viewport}`. Applied to ContextMenu, DropdownMenu, Popover, HoverCard, Tooltip, PendingMenuItem, Dialog.
- **Wide bars** get `cardMode:"column"`: Button, Input, Textarea, HunkBar, LineSelectionBar.
- Avatar: omit `email` (triggers offline gravatar fetch); renders colored-initials fallback.
- DiffLineMenu: wraps ContextMenu internally with no `open` prop exposed — only the trigger (a diff line) renders; menu content can't open statically. Graded good (trigger is the meaningful surface).

## Excluded-from-cards (26 — still importable via bundle barrel, componentSrcMap: null)
- Compound sub-parts that render blank alone (stay callable, documented in parent prompts): ContextMenuGroup/Label/RadioGroup/Separator/Shortcut, DialogFooter/Header, DropdownMenuGroup/Label/RadioGroup/Separator/Shortcut, TooltipProvider, TooltipHint.
- Un-previewable app-shell / mount-point / Tauri-store components: DragScrim, RepositoryPreviewCapture, RepositoryTabs, StageToggle, WindowControls, ErrorBoundary, Toaster (sonner mount), GithubContextPanel, ConfirmDialog, LogsModal, OnboardingModal, SettingRow.
- These render a non-empty-but-blank root so the floor-card swap (needs empty root / <2px) doesn't fire — excluding their card is the clean fix. `window.GitWyrm.*` still exports all 154; the @ds-bundle header lists 128.
- Note: several settings components (AboutSettings, AppearanceSettings, ChangeSizeSettings, ClearLogsButton) render REAL UI unauthored — they don't need Tauri to paint. Left as-is (render clean).

## Re-sync risks
- Compiled css filename is content-hashed; stage-css.mjs handles it but assumes exactly one `dist/assets/index-*.css`. If Vite output layout changes, update stage-css.mjs.
- componentSrcMap is enumerated, not discovered — a new component file won't appear until gen-srcmap.mjs is re-run and merged. Re-running gen-srcmap will re-ADD the 26 excluded components (they're real exports); the exclusions live as `null` values in config.json's componentSrcMap — merge-config.mjs preserves existing entries (existing wins), so keep the nulls. If you regenerate from scratch, re-apply the 26 exclusions.
- `.ds-build-meta.json`/bundle exclude list is derived from render-check `bad`; if a future build makes an excluded component render (e.g. a provider added), reconsider un-excluding it.
- Build is slow (~4 min for the 154-component ts-morph .d.ts pass). Run it backgrounded; never chain build+validate in one backgrounded command. Orphaned node processes from killed builds accumulated during this run — not a correctness issue but watch memory.
- Repo-specific helper scripts live in `.design-sync/scripts/` (COMMITTED, durable): `gen-barrel.mjs` (synth barrel entry), `gen-srcmap.mjs` (componentSrcMap regen), `merge-config.mjs` (merge srcmap preserving nulls), `stage-css.mjs` (compiled-css staging), `gen-upload-list.mjs` (upload chunking). `buildCmd` references these. The staged `.ds-sync/` copies are gitignored throwaways — the `.design-sync/scripts/` copies are the source of truth.
- Also update `.design-sync/ds-theme-surface.tsx` (the dark-surface provider, committed) is required — it's referenced by cfg.provider + cfg.extraEntries.
