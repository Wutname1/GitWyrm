---
name: GitWyrm
description: A calm control room built with the tactility and personality of a personal workbench.
colors:
  primary: "#1db584"
  primary-text: "#38b78e"
  primary-ink: "#04120d"
  canvas: "#121212"
  surface: "#1a1a1a"
  surface-raised: "#262626"
  surface-active: "#2d2d2d"
  border: "#2d2d2d"
  text: "#e5e5e5"
  text-secondary: "#9e9e9e"
  text-muted: "#717171"
  success: "#34d399"
  danger: "#f87171"
  warning: "#fbbf24"
  info: "#60a5fa"
  pull-request: "#a78bfa"
  graph-rose: "#fb7185"
  graph-lime: "#a3e635"
typography:
  title:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.25
  body:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 450
    lineHeight: 1.45
  label:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "0.6875rem"
    fontWeight: 600
    lineHeight: 1.45
    letterSpacing: "0.09em"
  mono:
    fontFamily: "Inter Variable, ui-monospace, SF Mono, monospace"
    fontSize: "0.6875rem"
    fontWeight: 600
    lineHeight: 1.4
  wordmark:
    fontFamily: "Sora, Inter Variable, system-ui, sans-serif"
    fontSize: "0.6875rem"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "0.085em"
rounded:
  status: "3px"
  compact: "4px"
  control: "5px"
  md: "6px"
  lg: "8px"
  xl: "12px"
  pill: "9999px"
spacing:
  micro: "4px"
  compact: "6px"
  control: "8px"
  group: "12px"
  panel: "16px"
  dialog: "24px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "8px 16px"
    height: "36px"
  button-secondary:
    backgroundColor: "{colors.surface-active}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "8px 12px"
    height: "32px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "6px 8px"
    height: "30px"
  input:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "4px 12px"
    height: "36px"
  repository-tab:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-secondary}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "0 8px"
    height: "36px"
  ref-chip:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-ink}"
    typography: "{typography.mono}"
    rounded: "{rounded.control}"
    padding: "1px 6px"
---

# Design System: GitWyrm

## Overview

**Creative North Star: "The Quiet Control Workbench"**

GitWyrm combines the calm situational awareness of a control room with the approachable tactility of a personal workbench. The interface is compact, capable, and information-rich, but each tool has a clear place and every state change answers the user visibly. It should feel made by people who care about the work, not standardized by an enterprise committee.

Personality lives in precise details: the Wyrm mark, colored graph paths, direct language, responsive presses, purposeful motion, and occasional wit. Those moments sit inside a disciplined shell so the repository remains the focus. The system is professional without becoming sterile, friendly without becoming toy-like, and dense without becoming cryptic.

**Key Characteristics:**

- Dark-first, theme-aware neutral surfaces with a restrained mint accent.
- Compact desktop density with resizable working panes and persistent context.
- Tactile controls with unmistakable hover, pressed, selected, pending, success, and failure states.
- Color used for meaning: action, Git topology, change status, warnings, and connected services.
- Small, deliberate moments of branded motion and wit inside an otherwise quiet workspace.

## Colors

The palette is a neutral tool bench illuminated by a small set of reliable signal colors. The frontmatter records the default Slate-dark first-paint values; runtime theme tokens preserve the same semantic roles across Slate, Onyx, Midnight, and Paper in dark and light modes.

### Primary

- **Deep Mint** (`primary`): the scarce action color for primary buttons, active edges, focus rings, progress, and important selection feedback.
- **Lit Mint** (`primary-text`): the calmer readable mint for accent text on dark and light surfaces.
- **Deep Mint Ink** (`primary-ink`): high-contrast content placed directly on mint fills.

### Secondary

- **Signal Blue** (`info`): informational emphasis, updates, and one graph lane.
- **Signal Purple** (`pull-request`): pull request identity and one graph lane.

### Tertiary

- **Signal Green** (`success`): added content and successful outcomes.
- **Signal Red** (`danger`): removed content, errors, destructive actions, and failure emphasis.
- **Signal Amber** (`warning`): modified content, waiting states, and warnings.
- **Lane Rose** and **Lane Lime** (`graph-rose`, `graph-lime`): topology colors reserved for keeping graph paths distinct.

### Neutral

- **Workbench Black** (`canvas`): the deepest application canvas.
- **Bench Charcoal** (`surface`): primary panels and structural regions.
- **Tool Steel** (`surface-raised`, `surface-active`): progressively lighter controls, hover fills, menus, and nested surfaces.
- **Tool Edge** (`border`): quiet separation where spacing and tonal contrast are not enough.
- **Workshop White**, **Soft Nickel**, and **Worn Steel** (`text`, `text-secondary`, `text-muted`): primary, supporting, and de-emphasized text.

### Named Rules

**The Signal, Not Paint Rule.** Deep Mint and semantic colors communicate action, state, or topology; they do not decorate empty space.

**The Selected Must Read Rule.** A selected control needs persistent contrast, an edge, a check, or another unmistakable marker. A faint tint alone is not a selected state.

**The Theme Contract Rule.** Components consume semantic variables, never a hard-coded theme surface, so every supported theme retains the same hierarchy and meaning.

## Typography

**Display Font:** Sora with Inter and system fallbacks, reserved for the GitWyrm wordmark.

**Body Font:** Inter Variable with system fallbacks by default. The user may choose IBM Plex Sans, Geist, Roboto, the system default, or an available local font.

**Label/Mono Font:** Inter Variable with monospaced fallbacks for paths, hashes, counts, timestamps, code, and compact status text.

**Character:** Type is compact, practical, and quiet. Weight and contrast establish hierarchy more often than dramatic size changes, keeping dense repository information scannable without shrinking below the established micro-type floor.

### Hierarchy

- **Title** (600, `title`): modal headings, major empty states, and the rare top-level screen heading.
- **Body** (450, `body`): controls, explanations, settings, and ordinary interface copy. The whole application can scale from 50% to 200%, and users can separately tune font family, size, and weight.
- **Label** (600, `label`): compact section headings and metadata labels, often uppercase when they divide a dense panel.
- **Mono** (600, `mono`): repository paths, branches, hashes, line numbers, counts, and code-adjacent metadata.
- **Wordmark** (600, `wordmark`): GitWyrm branding only.

### Named Rules

**The Quiet Hierarchy Rule.** Prefer weight, spacing, and text tone before increasing size; large type is exceptional in an operational desktop tool.

**The Micro Floor Rule.** Nothing renders below the established 0.6875rem micro size, and microcopy keeps generous line height.

## Layout

The application fills the native window and keeps its major context visible. A 36px repository strip, 48px toolbar, resizable left navigation pane, flexible center workspace, resizable right changes pane, and 24px status bar form the default desktop shell. The left pane defaults to 240px and the right to 320px, with user-controlled bounds; repository tabs can also move into a resizable vertical rail.

Spacing follows a compact 4/6/8/12/16/24px rhythm. Dense rows and controls use the lower steps, grouped content uses 12-16px, and modal interiors use 24px. Dividers and tonal surface shifts carry structure so padding can remain efficient. The product targets desktop work: it adapts through resizable panes, overflow, compact variants, application zoom, and alternate tab orientation rather than collapsing into a mobile composition.

**The Persistent Context Rule.** Repository, branch, change count, current worktree, and operation state stay visible whenever they help prevent work in the wrong place.

**The Resizable Workbench Rule.** Fixed rails establish orientation, but working panes belong to the user and retain useful minimum and maximum widths.

## Elevation & Depth

Tonal layering does most of the depth work. The canvas, primary panels, raised panels, and active surfaces step upward through close neutral values; thin borders clarify boundaries only where needed. Shadows are reserved for floating menus, dialogs, tooltips, drag ghosts, and temporary attention states. They are structural and ambient, never decorative chrome.

### Shadow Vocabulary

- **Field lift:** a nearly flat shadow on bordered inputs and outline controls so they remain tangible against a same-tone surface.
- **Menu lift:** a compact medium shadow for dropdowns and context menus that must detach from dense content.
- **Dialog lift:** a strong dark shadow beneath a modal shown over the 70% black overlay.
- **Tooltip lift:** a crisp ambient shadow plus a faint inner highlight so small hints remain legible over graph detail.
- **Attention ring:** an accent-colored inset edge or glow used briefly to reveal a destination, tutorial target, or completed focus action.

### Named Rules

**The Layer Before Shadow Rule.** Raise an ordinary surface with tone first; use a shadow only when the element truly floats or needs temporary attention.

## Shapes

The form language is gently machined rather than soft or bubbly. Most controls use 5-6px corners, compact badges tighten to 3-4px, dialogs and substantial cards open to 8-12px, and pills are reserved for branch references, statuses, progress, or compact temporary actions. Borders are usually one pixel and quiet. Circular geometry belongs to graph nodes, avatars, spinners, and icon-only status signals.

**The Radius Follows Scale Rule.** Small controls get small corners; large containers may be softer, but ordinary panels never become oversized floating capsules.

## Components

Components are compact, tactile, and unmistakably responsive. Every actionable element visibly changes on hover and press, while asynchronous actions add a spinner, pending label, progress treatment, or toast rather than going silent.

### Buttons

- **Shape:** gently machined corners (`rounded.md`) with heights from 24px for micro-actions to 40px for large actions; 30-36px is the ordinary desktop range.
- **Primary:** Deep Mint fill, Deep Mint Ink content, medium weight, and compact horizontal padding.
- **Hover / Focus:** hover changes brightness or fill; keyboard focus adds a three-pixel translucent mint ring and border; pressed states darken or return toward the underlying panel.
- **Secondary / Outline:** Tool Steel or canvas-toned fills with a quiet border and a lighter hover surface.
- **Ghost / Link:** no resting fill; hover adds a surface or underline, and pressed state remains visible.
- **Disabled / Pending:** disabled controls lose opacity and interaction; pending controls keep their footprint and replace or accompany the label with visible progress.

### Chips

- **Style:** branch and reference chips use compact monospaced text, 5px corners, and semantic fills. Status badges use outlined 3px squares. Pull requests, changes, and filters use restrained pill variants.
- **State:** selection, synchronization, drag source, and valid drop target each have a separate visible treatment. Long reference names may expand on hover instead of widening the graph column.

### Cards / Containers

- **Corner Style:** 6px for compact cards, 8-12px for dialogs and prominent transient panels.
- **Background:** tonal surfaces, normally Bench Charcoal or Tool Steel over Workbench Black.
- **Shadow Strategy:** flat at rest; shadow only when floating or demanding temporary attention.
- **Border:** a one-pixel Tool Edge when tonal contrast cannot define the boundary.
- **Internal Padding:** usually 8-16px; dialogs use 24px unless the surface owns a denser custom header and footer.

### Inputs / Fields

- **Style:** 36px default height, 6px corners, quiet border, transparent or canvas-toned fill, and selectable text.
- **Focus:** border shifts to Deep Mint and gains the same three-pixel translucent ring used by buttons.
- **Error / Disabled:** errors use Signal Red for border and ring; disabled fields keep their shape but reduce opacity and show the unavailable cursor.

### Navigation

Repository tabs, side-panel rows, settings navigation, and file tabs share compact labels and tonal hover states. Active repository tabs add a solid two-pixel edge on the side attached to the shell. Selected side rows combine a soft mint fill with a persistent mint edge or explicit indicator. Navigation may reveal secondary controls on hover, but keyboard focus must reveal them too.

### Commit Graph

The graph is GitWyrm's signature information surface. Six stable lane colors keep paths distinguishable; WIP and stash rows use distinct silhouettes and connectors rather than pretending to be ordinary commits. Branch and tag chips sit directly on the history they name. Selection, range selection, focus, dragging, and pending sync all answer with visible row, edge, chip, or motion feedback.

### Feedback and Motion

Motion explains cause and effect: sync direction drifts toward its result, selected destinations flash or pulse, drag targets light up, dialogs fade and scale together, and the Wyrm mark reacts when touched. Ordinary color changes are fast; dialogs use a coordinated 200ms transition; longer branded loops appear only during real waiting. Reduced-motion preferences remove nonessential transitions while preserving static emphasis.

## Do's and Don'ts

### Do:

- **Do** make hover, pressed, selected, pending, success, and failure visually distinct.
- **Do** preserve the layered neutral hierarchy and consume semantic theme variables.
- **Do** use Deep Mint sparingly for primary action, focus, progress, and important selection.
- **Do** keep repository context and operation state visible when an action could affect the wrong work.
- **Do** use compact spacing and type while respecting the micro-type floor and user scaling controls.
- **Do** allow small branded reactions and moments of wit when they reinforce a real action.

### Don't:

- **Don't** let any click, keypress, drag, or long-running action complete without a visible reaction.
- **Don't** use a faint dark-on-dark tint as the only selected-state marker.
- **Don't** decorate large areas with mint or graph colors; color must carry meaning.
- **Don't** add shadows to ordinary resting panels when tonal layering already separates them.
- **Don't** turn the interface into sterile enterprise software or a playful toy; keep capability and personality in balance.
- **Don't** introduce oversized cards, inflated spacing, or mobile-style capsules into the dense desktop workspace.
