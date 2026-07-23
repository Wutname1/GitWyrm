## GitWyrm conventions

GitWyrm is a **dark-only desktop git client** built with React + Tailwind v4 (shadcn "new-york" base). Style everything with the Tailwind utility classes below — this system ships a Tailwind preset, so you write utility classes, not inline styles or CSS. All components import from `window.GitWyrm.*`.

### Dark surface is required
Every screen sits on the dark app background. Put your layout on a root that carries the background and light text — the tokens resolve from `styles.css`:

```jsx
<div className="min-h-screen bg-background text-foreground">
  {/* your composition */}
</div>
```

`bg-background` is `#0b0f14`. Building on a white/default surface will make inputs, muted text, and ghost buttons wash out — always start from `bg-background`.

### Color utility families (real classes from this DS's Tailwind preset)
Use these instead of inventing hex values or generic Tailwind grays:

| Purpose | Utilities |
|---|---|
| App / surfaces | `bg-background` (app), `bg-panel` `bg-panel2` `bg-panel3` (nested panels, lighter as they stack), `bg-modal` (dialogs), `bg-popover` |
| Text | `text-foreground` (primary), `text-sub` (secondary), `text-muted-foreground` (dim), `text-accent-text` (accent-colored text — use this, not `text-primary`, for green text) |
| Accent | `bg-primary` + `text-primary-foreground` (the green fill for primary buttons/badges), `bg-soft` (dim accent wash for highlights/selection), `text-accent-text` |
| Borders | `border-border` (default), `border-primary` (accent), `border-removed` (red) |
| Diff / git status | `text-added` `bg-added` (green, additions), `text-removed` `bg-removed` (red, deletions), `text-modified` (amber). Graph lanes exist as `--gw-lane0`…`--gw-lane5` custom properties |
| Semantic | `bg-secondary` `text-secondary-foreground`, `bg-destructive` (red) |

Type floor: nothing renders below `text-2xs` (0.6875rem). Fonts are wired by the DS — sans is IBM Plex Sans Variable, headings/wordmark use Sora, monospace uses Geist Mono (via the `font-mono` stack). Don't set `font-family`.

### Components
- Buttons: `<Button variant="default|secondary|outline|ghost|destructive|link" size="default|xs|sm|lg|icon|icon-sm|icon-lg">`. A `tooltip` prop adds a hover tooltip. Icons come from `lucide-react` and go inside the button as children.
- Compounds follow shadcn patterns: `Dialog`/`DialogContent`/`DialogHeader`/`DialogTitle`/`DialogFooter`; `DropdownMenu`/`DropdownMenuTrigger`/`DropdownMenuContent`/`DropdownMenuItem`; same shape for `ContextMenu`, `Popover`, `HoverCard`, `Collapsible`. All are on `window.GitWyrm.*`.
- Tooltips need a `TooltipProvider` ancestor (or use the `tooltip` prop on `Button`/`TooltipButton`).
- Git-domain building blocks exist: `Avatar`, `StatusBadge` (status code `A`/`M`/`D`/`R`/`!`), `ChangeSizeIndicator`, `HunkBar`, `CommitRow`, `RefBadge`, `SectionItemRow`, `TabBar`, `StatusBar`. Read their `.prompt.md` + `.d.ts` before use.

### Where the truth lives
Read the real files before styling: `styles.css` and its imports (the token/utility source), and `components/<group>/<Name>/<Name>.prompt.md` + `.d.ts` for any component's exact API and example usage.

### Idiomatic snippet

```jsx
const { Button, StatusBadge } = window.GitWyrm;

<div className="min-h-screen bg-background text-foreground p-4">
  <div className="flex items-center gap-2 rounded-md bg-panel2 border border-border p-3">
    <StatusBadge st="M" />
    <span className="text-sub text-sm">src/lib/refSync.ts</span>
    <span className="ml-auto text-added text-2xs">+42</span>
    <span className="text-removed text-2xs">-8</span>
    <Button size="sm">Stage</Button>
  </div>
</div>
```
