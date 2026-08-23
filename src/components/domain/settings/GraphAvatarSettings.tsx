import { laneColor } from '@/lib/gitDisplay'
import { useWorkspaceStore } from '@/stores/workspaceStore'

/** Node geometry, mirroring the graph's own avatar nodes. */
const OUTER_R = 11
const RING = 2
const R = OUTER_R - RING
/** Lane pitch, matching the graph's widened avatar spacing. */
const PITCH = OUTER_R * 2 + 2

/**
 * The avatar toggle, with a small picture of what each choice looks like.
 *
 * The preview matters more than usual here: "commit dots become pictures" is
 * easy to read as "the lane colors go away", which is not what happens. Seeing
 * the ring answers that without anyone having to switch the setting on to find
 * out.
 */
export function GraphAvatarSettings() {
  const showAvatars = useWorkspaceStore((s) => s.showGraphAvatars)
  const setShowAvatars = useWorkspaceStore((s) => s.setShowGraphAvatars)

  return (
    <div className="flex max-w-xl flex-col gap-3">
      <label className="flex cursor-pointer items-center gap-2 text-xs text-foreground">
        <input
          type="checkbox"
          checked={showAvatars}
          onChange={(event) => setShowAvatars(event.target.checked)}
          className="size-3.5 accent-[var(--gw-accent)]"
        />
        Show who wrote each commit in the graph
      </label>

      <div className="rounded-md border border-border bg-background p-3">
        <div className="mb-2 text-2xs font-semibold uppercase tracking-[.05em] text-muted-foreground">
          Live example
        </div>
        <svg
          width={14 + 2 * PITCH + OUTER_R + 2}
          height={64}
          role="img"
          aria-label={showAvatars ? 'Commits drawn as pictures with a colored ring' : 'Commits drawn as colored dots'}
        >
          <defs>
            {[0, 1, 2].map((lane) => (
              <clipPath key={lane} id={`gw-avatar-preview-${lane}`}>
                <circle cx={14 + lane * PITCH} cy={32} r={R} />
              </clipPath>
            ))}
          </defs>
          {[0, 1, 2].map((lane) => {
            const x = 14 + lane * PITCH
            const col = laneColor(lane)
            return (
              <g key={lane}>
                <path d={`M ${x} 8 L ${x} 56`} stroke={col} strokeWidth={2.25} fill="none" strokeLinecap="round" />
                {showAvatars ? (
                  <>
                    <circle cx={x} cy={32} r={OUTER_R} fill="var(--gw-bg)" />
                    {/* A face stands in for the real picture: the setting has
                        to preview correctly before anyone is signed in, and
                        with no repo open there is no author to look up. */}
                    <circle cx={x} cy={32} r={R} fill={col} opacity={0.28} />
                    <g clipPath={`url(#gw-avatar-preview-${lane})`} fill={col} opacity={0.75}>
                      <circle cx={x} cy={29.6} r={2.8} />
                      <circle cx={x} cy={40} r={5.4} />
                    </g>
                    <circle
                      cx={x}
                      cy={32}
                      r={OUTER_R - RING / 2}
                      fill="none"
                      stroke={col}
                      strokeWidth={RING}
                    />
                  </>
                ) : (
                  <circle cx={x} cy={32} r={6} fill={col} stroke="var(--gw-bg)" strokeWidth={2} />
                )}
              </g>
            )
          })}
        </svg>
        <p className="mt-2 text-2xs leading-relaxed text-muted-foreground">
          {showAvatars
            ? 'Each dot becomes the author’s picture. The branch color stays as a ring around it.'
            : 'Each commit is a dot in its branch color.'}
        </p>
      </div>

      <p className="text-2xs leading-relaxed text-muted-foreground">
        Pictures come from GitHub or Gravatar, matched on the email in the commit. Authors with no
        picture keep a plain dot.
      </p>
    </div>
  )
}
