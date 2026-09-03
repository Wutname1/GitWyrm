---
name: GitWyrm
description: A warm-dark workshop for Git and AI agents; dense, calm, alive only where work is happening.
colors:
  deep-mint: "#1db584"
  mint-text: "#38b78e"
  mint-ink: "#04120d"
  mint-haze: "rgba(29, 181, 132, 0.13)"
  workshop-floor: "#121212"
  bench: "#1a1a1a"
  bench-raised: "#262626"
  bench-edge: "#2d2d2d"
  chalk: "#e5e5e5"
  chalk-dim: "#9e9e9e"
  chalk-faint: "#717171"
  signal-green: "#34d399"
  signal-red: "#f87171"
  signal-amber: "#fbbf24"
  signal-blue: "#60a5fa"
  signal-purple: "#a78bfa"
  lane-mint: "#2dd4a7"
  lane-sky: "#38bdf8"
  lane-amber: "#f59e0b"
  lane-violet: "#a78bfa"
  lane-rose: "#fb7185"
  lane-lime: "#a3e635"
typography:
  wordmark:
    fontFamily: "Sora, Inter Variable, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1
  title:
    fontFamily: "IBM Plex Sans Variable, system-ui, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 600
    lineHeight: 1
  headline:
    fontFamily: "IBM Plex Sans Variable, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.25
  body:
    fontFamily: "IBM Plex Sans Variable, system-ui, sans-serif"
    fontSize: "0.84375rem"
    fontWeight: 450
    lineHeight: 1.45
  label:
    fontFamily: "IBM Plex Sans Variable, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.35
  micro:
    fontFamily: "IBM Plex Sans Variable, system-ui, sans-serif"
    fontSize: "0.6875rem"
    fontWeight: 600
    lineHeight: 1.45
    letterSpacing: "0.04em"
  mono:
    fontFamily: "Geist Mono, ui-monospace, SF Mono, monospace"
    fontSize: "0.6875rem"
    fontWeight: 500
    lineHeight: 1.45
rounded:
  badge: "3px"
  icon: "4px"
  tab: "5px"
  sm: "0.25rem"
  md: "0.375rem"
  lg: "0.5rem"
  xl: "0.75rem"
spacing:
  hairline: "1px"
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  row: "28px"
  row-tall: "42px"
  control: "30px"
components:
  button-primary:
    backgroundColor: "{colors.deep-mint}"
    textColor: "{colors.mint-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "0 16px"
    height: "36px"
  button-primary-hover:
    backgroundColor: "rgba(29, 181, 132, 0.9)"
    textColor: "{colors.mint-ink}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.chalk-dim}"
    rounded: "{rounded.md}"
    padding: "0 12px"
    height: "32px"
  button-ghost-hover:
    backgroundColor: "{colors.bench-edge}"
    textColor: "{colors.chalk}"
  button-link:
    backgroundColor: "transparent"
    textColor: "{colors.mint-text}"
  button-xs:
    backgroundColor: "{colors.bench-raised}"
    textColor: "{colors.chalk}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "0 8px"
    height: "24px"
  input:
    backgroundColor: "transparent"
    textColor: "{colors.chalk}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "4px 12px"
    height: "36px"
  input-search:
    backgroundColor: "{colors.bench-raised}"
    textColor: "{colors.chalk}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "0 10px"
    height: "{spacing.control}"
  panel:
    backgroundColor: "{colors.bench}"
    textColor: "{colors.chalk}"
  panel-raised:
    backgroundColor: "{colors.bench-raised}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.lg}"
    padding: "6px"
  popover:
    backgroundColor: "{colors.bench-raised}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.md}"
    padding: "16px"
  menu:
    backgroundColor: "{colors.bench-raised}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.md}"
    padding: "4px"
  tooltip:
    backgroundColor: "{colors.bench-edge}"
    textColor: "{colors.chalk}"
    typography: "{typography.micro}"
    rounded: "{rounded.tab}"
    padding: "6px 10px"
  dialog:
    backgroundColor: "{colors.bench}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.lg}"
    padding: "24px"
  tab-active:
    backgroundColor: "{colors.mint-haze}"
    textColor: "{colors.chalk}"
    rounded: "{rounded.tab}"
  chip-status:
    backgroundColor: "transparent"
    textColor: "{colors.chalk-dim}"
    typography: "{typography.mono}"
    rounded: "{rounded.badge}"
    size: "16px"
  choice-card:
    backgroundColor: "transparent"
    textColor: "{colors.chalk}"
    rounded: "{rounded.md}"
    padding: "8px 10px"
  choice-card-selected:
    backgroundColor: "{colors.mint-haze}"
    textColor: "{colors.chalk}"
---

# Design System: GitWyrm

## Overview

**Creative North Star: "The Night Workshop"**

GitWyrm looks like a workbench after hours. The surfaces are warm-dark, not cold-dark:
a near-black floor (#121212) with benches stepping up in tone rather than in shadow, so
a panel sits on the floor the way a tool sits on a bench, by being a shade lighter. A
lot is laid out within reach (rows are 28 pixels, the smallest text is 11 pixels, the
sidebar holds dozens of chats and branches) and none of it shouts. One signal light,
Deep Mint, marks the thing that matters right now: the selected tab's edge, the one
filled button, the ring around a drop target, the pulse on a sync in progress.

The system is alive only in small places. Motion and glow appear where work is
happening and nowhere else: a sync badge pulses, the AI stage wakes, a dragged tab
finds its target, the logo springs when someone clicks it for fun. Everything at rest
is still. Copy is honest and plain: state is shown as it is, in words a non-expert can
read, and a control that cannot act says why rather than pretending.

It is not a raw terminal. Monospace is a detail (a SHA, a count, a badge), never the
voice; hierarchy comes from weight, size and tone, not from ALL CAPS and neon on black.
Four surface themes (Slate, Onyx, Midnight, Paper) and a light mode exist, but the
identity is the dark Slate workshop with the mint light on.

**Key Characteristics:**
- Warm-dark tonal layering: four surface steps, one hairline border, no resting shadows.
- One accent, two tokens: Deep Mint for fills and edges, a calmer Mint Text for words.
- Dense rows, small calm type, an 11-pixel floor that nothing goes under.
- Motion and glow only where something is happening.
- Bright categorical palettes (graph lanes, authors, tab groups) that identify and are
  never confused with the accent.

## Colors

A near-neutral dark ramp under one mint signal, with a small set of fixed status colors.

### Primary
- **Deep Mint** (#1db584): fills and edges only. The primary button, the active tab's
  2px edge, the focus ring, the drop-target ring, the sync pulse. Runtime themes derive
  it in OKLCH; the hex is the design's canonical value and the first-paint default.
- **Mint Text** (#38b78e): every green word. Links, the accent icon on a selected row,
  the "Change" affordance. Slightly lighter than the fill so it does not glare at ~10:1
  on the floor.
- **Mint Ink** (#04120d): text on a mint fill.
- **Mint Haze** (rgba 29,181,132 at 13%): the selected tab, a selected choice card, a
  hovered tab group; the only tinted surface in the system.

### Neutral
- **Workshop Floor** (#121212): the app background.
- **Bench** (#1a1a1a): panels, the modal body, cards at rest.
- **Bench Raised** (#262626): popovers, menus, composer and secondary surfaces, the
  search field, hover fill on list rows.
- **Bench Edge** (#2d2d2d): the hairline border everywhere, tooltip body, the strongest
  hover fill (ghost buttons, icon buttons).
- **Chalk** (#e5e5e5): primary text.
- **Chalk Dim** (#9e9e9e): secondary labels, sidebar rows at rest, toolbar labels.
- **Chalk Faint** (#717171): placeholders, metadata, disabled and icon-only controls at
  rest.

### Tertiary (status and identity)
- **Signal Green** (#34d399): added lines, success, a tab whose repo has pending work.
- **Signal Red** (#f87171): removed lines, errors, the destructive button, a failed
  panel's warmed border.
- **Signal Amber** (#fbbf24): modified lines, warnings, a card that needs attention.
- **Signal Blue** (#60a5fa): info.
- **Signal Purple** (#a78bfa): pull requests, everywhere they appear, so the icon alone
  identifies the kind.
- **Lane palette** (#2dd4a7, #38bdf8, #f59e0b, #a78bfa, #fb7185, #a3e635): commit-graph
  lanes; a second bright palette colours author avatars and tab groups. These identify
  things and stay bright on purpose.

### Named Rules
**The Two-Token Mint Rule.** Mint fills use Deep Mint; mint words use Mint Text. Never
set text in the fill colour, and never use `text-primary` for green text.

**The Signal Light Rule.** The accent marks what is selected, focused, dropping or
running. It never decorates. If two things on one screen are mint and neither is the
current thing, one of them is wrong.

**The Identity Palette Rule.** Lane, author and tab-group colours are for telling things
apart. They may be brighter than the accent, and they are never used as an accent.

## Typography

**Display Font:** Sora SemiBold (wordmark only; falls back to Inter Variable, system-ui)
**Body Font:** IBM Plex Sans Variable (default; the person may switch to Inter, Geist,
Roboto or the system font in Appearance, and the whole scale follows in rem)
**Label/Mono Font:** Geist Mono (SHAs, counts, badges, paths)

**Character:** Plain and even. Plex at a 450 weight and 13.5px reads like careful
handwriting on a label rather than a headline; the wordmark is the one place the type
has a personality of its own. Hierarchy is carried by weight (450 to 600) and a small
size scale, never by colour or capitals alone.

### Hierarchy
- **Wordmark** (600, 1rem, tight): the GitWyrm mark in the title bar and the AI stage.
- **Title** (600, 1.125rem, 1): dialog titles.
- **Headline** (600, 1rem, 1.25): the one heading on an empty state or landing pane.
- **Body** (450, 0.84375rem, 1.45): the base label size for everything inherited; the
  transcript, list rows, descriptions. Max reading width about 65ch in prose panels.
- **Label** (500, 0.75rem, 1.35): control labels, toolbar text, chips, small headings.
- **Micro** (600, 0.6875rem, 1.45, uppercase with 0.04em tracking when used as a section
  eyebrow): the smallest size anywhere. Section eyebrows, tooltips, badge text,
  timestamps.
- **Mono** (500, 0.6875rem): SHAs, counts, file paths, the sync badge number.

### Named Rules
**The 11px Floor Rule.** Nothing renders below `text-2xs` (0.6875rem). Need it smaller?
It should not be on screen.

**The Rem Rule.** Every font size is rem. The Text Size setting scales the root; a
pixel size anywhere breaks that promise.

## Layout

A fixed desktop shell: title bar with repository tabs (28px rows, draggable, grouped),
a toolbar of 30px controls, a left panel (branches, remotes, stashes, or the Agent
Desk chat list), a centre view (the virtualized commit graph at 28px per row, 42px when
a row shows its change size; the diff; the Agent Desk conversation), a right panel or
docked detail, and a status bar. Nothing scrolls the page; every region scrolls
itself.

Density is the default. Horizontal padding steps 8, 10, 12 and 16px; vertical gaps 4
and 6px inside a row, 8 and 12px between blocks. Rows are hit-tested at their full
28px height. Panels are separated by the hairline border, not by gaps, so the shell
reads as one instrument rather than floating cards.

Windows are resizable down to about 720 by 560; below 900px wide a right-docked detail
becomes a bottom dock and popovers take over. There is no phone layout: this is a
desktop application.

## Elevation & Depth

Depth is tonal. Four surface steps (Floor, Bench, Bench Raised, Bench Edge) stack by
lightness, separated by the one hairline border. Nothing at rest carries a shadow.

Shadows exist only for things that genuinely float above the shell, and only while
they float:

### Shadow Vocabulary
- **Floating menu** (`box-shadow: shadow-md`, Tailwind's medium): popovers and dropdown
  menus, on Bench Raised.
- **Tooltip** (`0 8px 24px rgba(0,0,0,0.42), inset 0 1px 0 rgba(255,255,255,0.04)`): the
  inset hairline is the "lit top edge" of a small object on the bench.
- **Dialog** (`shadow-lg` at 60% black over a 70% black overlay): modals, on the Bench
  modal surface one step above the floor.
- **Progress panel** (`0 10px 30px rgba(0,0,0,0.45)`, `0 14px 40px rgba(0,0,0,0.6)` when
  it has failed and warmed to red).
- **Lifted chip** (`0 2px 10px rgb(0 0 0 / 0.45)`): a branch chip expanding over the
  graph on hover.
- **Signal glow**: not a shadow in the depth sense. The drop-target ring is
  `0 0 0 2px Deep Mint` with a pulsing 45% halo; the AI stage's eye pulses a 5 to 9px
  mint glow; a tab with pending work pulses a 3px green ring. These say "here, now".

### Named Rules
**The Tone-Not-Shadow Rule.** If it is part of the shell, it is flat. If it floats
(menu, tooltip, dialog, dragged thing), it may cast one shadow. Cards never get shadows
to look important.

## Shapes

Small, precise corners: 6px (`--radius`, 0.375rem) on controls, inputs, menus and
choice cards; 8px on dialogs and raised panels; 12px only on the progress panel; 5px on
repository tabs and tooltips; 4px on icon tiles and toast buttons; 3px on the 16px status
badge. Fully round is reserved for the sync count pill and avatar rings.

Borders are one hairline of Bench Edge, everywhere, including on transparent inputs.
Selection is an edge, not a fill: the active tab carries a 2px accent bar along the
side it is attached to (top in the strip, left in the rail); inside a tab group that bar
takes the group's colour. Focus is a 3px ring of Deep Mint at 50%.

## Components

### Buttons
- **Shape:** gently rounded (6px); heights 24 (xs), 32 (sm), 36 (default), 40 (lg); icon
  buttons are square at the same heights.
- **Primary:** Deep Mint fill, Mint Ink text, 16px horizontal padding, 500-weight label
  text; hover dims the fill to 90%. One per screen: the thing you are here to do
  (Commit, Send, Allow it, Keep).
- **Secondary:** Bench Edge fill, Chalk text; hover to 80%.
- **Outline:** hairline border on the floor colour, hover fills Bench Edge.
- **Ghost:** no fill, Chalk Dim text; hover fills Bench Edge and brightens to Chalk. The
  workhorse for toolbars and row actions.
- **Destructive:** Signal Red fill, white text, 60% in dark mode so it does not glare.
- **Link:** Mint Text, underline on hover.
- **Focus:** 3px ring of Deep Mint at 50%, border turns Deep Mint. Disabled: 50% opacity,
  no pointer.

### Chips
- **Status badge:** a 16px square, 3px corners, hairline border, one mono bold letter
  (M, A, D) in the matching signal colour.
- **Branch chip:** on the graph row; expands over the graph on hover with the lifted
  chip shadow instead of forcing the column wider.
- **Choice card** (Agent Desk landing): a 6px bordered tile with a 12px title and a
  10.5px description; selected state swaps the border for Deep Mint at 60% and fills
  with Mint Haze.

### Cards / Containers
- **Corner Style:** 8px for raised panels and dialogs, 6px for inline cards.
- **Background:** Bench Raised for the composer and detail cards; Bench for panels.
- **Shadow Strategy:** none at rest (Tone-Not-Shadow Rule).
- **Border:** the hairline.
- **Internal Padding:** 6px on the composer shell, 12px on detail cards, 24px in dialogs.
- **Attention state:** an amber hairline border and a Bench Raised detail block
  (start-failure cards); a red-warmed border when something failed.

### Inputs / Fields
- **Style:** transparent field, hairline border, 6px corners, 36px tall, 12px
  horizontal padding; the toolbar search is a 30px Bench Raised field with a 14px icon.
- **Focus:** border to Deep Mint plus the 3px 50% ring; the search field only brightens
  its border to Chalk Faint.
- **Error / Disabled:** red border with a 20% red ring; 50% opacity.
- **Textarea:** grows with content (`field-sizing: content`), minimum 64px; the commit
  description grows from two lines to five before it scrolls.

### Navigation
- **Repository tabs:** 28px rows, 5px corners, Chalk Dim text; hover fills Bench
  Raised; the active tab fills Mint Haze and carries the 2px accent edge. Tab groups
  tint their whole strip 12% of the group colour when their handle is hovered.
- **Sidebar rows:** full-height hit areas, 12px text, hover fills Bench Raised, selected
  fills Mint Haze with Chalk text and a Mint Text icon.
- **Toolbar:** ghost buttons with 12px labels and 14px icons; the sync button carries a
  mono count pill in Deep Mint that pulses while a sync runs.
- **Docked details:** Source, Context and Graph open as popovers from icon buttons in
  the chat header, and can be pinned right, bottom, or beside the chat list.

### Feedback
- **Toasts:** dark, with the severity carried only by the icon colour and a 3px left
  edge (red, amber, green, blue); a copy and close rail on the right at 24px. Never a
  fully coloured toast.
- **Tooltips:** Bench Edge body, 5px corners, micro type, the lit-edge shadow, a 10px
  arrow.
- **Dialogs:** centred, 8px corners, 24px padding, 200ms fade and 95% zoom in and out
  over a 70% black overlay; the overlay and panel share the same duration on purpose.

### Signature: the signal glow and the wyrm
The AI stage (commit-message generation, agent progress) is the one place the system
performs: an 82px logo column that wakes in 320ms and then "thinks" on a 2.6s loop, a
mint-tinted hairline border, and node lights that pulse a 5 to 9px glow. The logo
itself springs (0.42s, a springy cubic-bezier) when clicked and, on rapid clicks,
blasts into shards. Every one of these respects `prefers-reduced-motion`.

## Do's and Don'ts

### Do:
- **Do** use `text-accent-text` for any green word and `bg-primary` / `border-primary`
  for any mint fill or edge (the Two-Token Mint Rule).
- **Do** keep every font size in rem and never below `text-2xs`.
- **Do** separate regions with the hairline border and a tone step, not with gaps or
  shadows.
- **Do** put motion only on a thing that is happening: a pulse while it runs, a glow
  where it lands, and stop it when it stops.
- **Do** reserve the filled mint button for the one action a screen exists for, and
  make everything else ghost or secondary.
- **Do** write labels a non-expert can read, and let a control that cannot act say why
  on hover.

### Don't:
- **Don't** set text in Deep Mint or use `text-primary` for words; it glares.
- **Don't** add shadows to cards or panels at rest, or use a shadow to make something
  look important.
- **Don't** reach for monospace as a voice. It marks a SHA, a count, a path; the raw
  terminal is the anti-reference.
- **Don't** use a lane, author or tab-group colour as an accent, or dull them to match
  the accent; they identify.
- **Don't** put a second mint highlight on a screen that already has a current thing.
- **Don't** gate an action behind typing a word to confirm, and never leave an action
  without a visible response.
