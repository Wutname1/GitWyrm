import { describe, it, expect } from 'vitest'
import githubIcon from '@/assets/icons/github.svg?raw'
import copilotIcon from '@/assets/icons/githubcopilot.svg?raw'
import openaiIcon from '@/assets/icons/openai.svg?raw'
import grokIcon from '@/assets/icons/grok.svg?raw'
import renovateIcon from '@/assets/icons/renovate.svg?raw'

/**
 * The contract behind every silhouette mark, pinned because breaking it is
 * invisible in review and produced a black-on-black GitHub logo in the graph.
 *
 * These files paint only with `currentColor`, which has one consequence that is
 * easy to forget: the art carries NO luminance of its own. Rendered anywhere it
 * cannot inherit a color -- inside an SVG `<image>`, which is its own document
 * -- it comes out solid black.
 *
 * So the shape lives entirely in the alpha channel, and any mask built from one
 * of these must read alpha. An SVG `<mask>` defaults to *luminance*, which
 * measures a black glyph as zero and masks the whole node away. That is exactly
 * what happened in `GraphSvg`; the author column was unaffected because CSS
 * `mask-image` is alpha-based already.
 *
 * If an icon ever ships with real colors baked in, this fails and the masking
 * decision has to be revisited rather than silently inherited.
 */
const MONO_SOURCES: ReadonlyArray<readonly [string, string]> = [
  ['github', githubIcon],
  ['githubcopilot', copilotIcon],
  ['openai', openaiIcon],
  ['grok', grokIcon],
  ['renovate', renovateIcon],
]

describe('silhouette marks', () => {
  it.each(MONO_SOURCES)('%s paints only with currentColor, so it has no luminance', (_name, svg) => {
    const fills = [...svg.matchAll(/fill="([^"]+)"/g)].map((m) => m[1])
    expect(fills.length).toBeGreaterThan(0)
    // Any hard-coded color means this is no longer a silhouette, and the
    // alpha-mask reasoning above stops applying to it.
    expect(fills.every((f) => f === 'currentColor')).toBe(true)
  })
})
