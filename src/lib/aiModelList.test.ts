import { describe, expect, it } from 'vitest'
import type { CatalogModel } from '@/lib/bindings'
import { AUTO_MODEL_ID, canAutoSelectFrom, modelListCaveat, onlyAutoOffered } from './aiModelList'

/**
 * The defect these exist for: a live Copilot model list containing nothing but
 * `auto` was treated as a reading of the account's entitlements.
 *
 * It is the documented reply to a call whose token was not honoured, it comes
 * back well-formed and marked live, and on that evidence GitWyrm overwrote a
 * model the person had deliberately chosen -- silently, into the persisted
 * store, where it outlived the bad response because `auto` then counted as a
 * valid saved selection.
 */

const model = (id: string, enabled = true): CatalogModel => ({ id, name: id, enabled })

describe('onlyAutoOffered', () => {
  it('is false for an ordinary list', () => {
    expect(onlyAutoOffered([model(AUTO_MODEL_ID), model('claude-sonnet-4.5')])).toBe(false)
  })

  it('is true when auto is the only thing on offer', () => {
    expect(onlyAutoOffered([model(AUTO_MODEL_ID)])).toBe(true)
  })

  it('is false for an empty list, which is a different problem with its own handling', () => {
    expect(onlyAutoOffered([])).toBe(false)
  })

  // The static catalog marks everything enabled, and a live list can carry
  // models the plan excludes. Only what the user could actually pick counts.
  it('ignores models the account cannot use', () => {
    expect(onlyAutoOffered([model(AUTO_MODEL_ID), model('gpt-5', false)])).toBe(true)
  })
})

describe('canAutoSelectFrom', () => {
  it('allows picking for the user from a real list', () => {
    expect(canAutoSelectFrom([model('claude-sonnet-4.5')])).toBe(true)
  })

  // The heart of it: never overwrite a deliberate choice on this evidence.
  it('refuses to pick for the user when only auto came back', () => {
    expect(canAutoSelectFrom([model(AUTO_MODEL_ID)])).toBe(false)
  })

  it('refuses when nothing is usable at all', () => {
    expect(canAutoSelectFrom([model('gpt-5', false)])).toBe(false)
  })
})

describe('modelListCaveat', () => {
  it('says nothing about a list that speaks for itself', () => {
    expect(modelListCaveat([model('claude-sonnet-4.5')])).toBeNull()
  })

  it('explains an only-auto list without claiming to know which cause applies', () => {
    const text = modelListCaveat([model(AUTO_MODEL_ID)])
    expect(text).not.toBeNull()
    // Names what was seen, and leaves auto standing as usable rather than
    // telling someone their setup is broken when it demonstrably works.
    expect(text).toContain('Auto')
    expect(text).toContain('usually')
    // Never asserts the account is small, which GitWyrm cannot see from here.
    expect(text).not.toMatch(/your plan (is|only)/i)
  })
})
