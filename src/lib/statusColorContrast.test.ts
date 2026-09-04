import { describe, expect, it } from 'vitest'
import { STATUS_COLORS } from './themes'

/**
 * The five status colours have to be readable on the background they sit on.
 *
 * They used to be declared once in `index.css` and never rewritten, so a light
 * theme kept the dark-tuned pastels: amber measured 1.67:1 against a white
 * panel, well under the 4.5:1 that text needs, and amber is the colour that
 * warns. These tests do the measurement rather than trusting the eye.
 */
const hex = (h: string): [number, number, number] => {
  const n = parseInt(h.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}
const channel = (c: number): number => {
  const s = c / 255
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}
const luminance = ([r, g, b]: [number, number, number]): number =>
  0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
const contrast = (a: string, b: string): number => {
  const [hi, lo] = [luminance(hex(a)), luminance(hex(b))].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

const AA = 4.5

describe('status colour contrast', () => {
  it('is readable on a light panel in light mode', () => {
    // White is the brightest panel any shipped light theme uses, so it is the
    // hardest case.
    for (const [name, value] of Object.entries(STATUS_COLORS.light)) {
      expect(contrast(value, '#ffffff'), `${name} on white`).toBeGreaterThanOrEqual(AA)
    }
  })

  it('is readable on a dark panel in dark mode', () => {
    for (const [name, value] of Object.entries(STATUS_COLORS.dark)) {
      expect(contrast(value, '#1a1a1a'), `${name} on the dark panel`).toBeGreaterThanOrEqual(AA)
    }
  })

  it('does not reuse a dark value in light mode', () => {
    // The whole defect was one set of values serving both modes.
    for (const key of Object.keys(STATUS_COLORS.dark) as (keyof typeof STATUS_COLORS.dark)[]) {
      expect(STATUS_COLORS.light[key]).not.toBe(STATUS_COLORS.dark[key])
    }
  })
})
