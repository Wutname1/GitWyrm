/**
 * The archive-box the graph draws for a stash node, for use outside the graph.
 * The paths are the graph's unselected node geometry verbatim (a 14x12 box on
 * the lane center, lid line, latch), scaled by the viewBox, so the sidebar row
 * reads as the same symbol. The graph keeps its own copy inline because its
 * node also grows when selected; if that geometry changes, change it here too.
 */
export function StashGlyph({ size = 12, color }: { size?: number; color: string }) {
  return (
    <svg
      aria-hidden
      width={size}
      height={size}
      viewBox="-8 -8 16 16"
      fill="var(--gw-bg)"
      stroke={color}
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x={-7} y={-6} width={14} height={12} rx={2} />
      <path d="M -7 -2 H 7" />
      <path d="M -2 1 H 2" />
    </svg>
  )
}
