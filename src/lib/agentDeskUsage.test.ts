import { describe, expect, it } from 'vitest'
import { buildAgentUsageLines, explainUsageUnavailable, buildUsageRows, hasAnyUsageData, nodeUsageLine } from './agentDeskUsage'
import type { AgentUsageRow, SessionUsage } from '@/lib/bindings'

const EMPTY: SessionUsage = {
  sessionTokens: null,
  sessionRequests: null,
  sessionCostUsd: null,
  planLimit: null,
  planResetAt: null,
  activeHelperCount: null,
  contextUsed: null,
  contextSize: null,
  agents: [],
  dataTimestamp: '2026-08-19T00:00:00Z',
}

const LEAD: AgentUsageRow = {
  executionId: 'lead-1',
  inputTokens: null,
  outputTokens: null,
  label: 'Lead',
  isLead: true,
  tokens: 1200,
  costMicroUsd: 4500,
  turns: 3,
}

const HELPER: AgentUsageRow = {
  executionId: 'helper-1',
  inputTokens: null,
  outputTokens: null,
  label: 'Trace the crash',
  isLead: false,
  tokens: 500,
  costMicroUsd: null,
  turns: 1,
}

describe('buildAgentUsageLines', () => {
  it('shows nothing when a lone lead would only repeat the totals', () => {
    expect(buildAgentUsageLines({ ...EMPTY, agents: [LEAD] })).toEqual([])
    expect(buildAgentUsageLines(EMPTY)).toEqual([])
    // Older session files predate the field entirely.
    expect(buildAgentUsageLines({ ...EMPTY, agents: undefined })).toEqual([])
  })

  it('lists every agent once a helper exists', () => {
    const lines = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, HELPER] })
    expect(lines.map((l) => l.label)).toEqual(['Lead', 'Trace the crash'])
    expect(lines[0].parts).toEqual(['1.2k tokens', '3 turns', '$0.0045'])
  })

  it('shows a lone helper too, since the lead may not have reported yet', () => {
    expect(buildAgentUsageLines({ ...EMPTY, agents: [HELPER] })).toHaveLength(1)
  })

  it('never invents a zero for a figure an agent did not report', () => {
    const lines = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, HELPER] })
    expect(lines[1].parts).toEqual(['500 tokens', '1 turn'])
    const silent: AgentUsageRow = { ...HELPER, executionId: 'helper-2', tokens: null, turns: null }
    const quiet = buildAgentUsageLines({ ...EMPTY, agents: [LEAD, silent] })
    expect(quiet[1].parts).toEqual([])
  })
})

describe('buildUsageRows', () => {
  it('produces no rows at all when every field is unknown', () => {
    expect(buildUsageRows(EMPTY)).toEqual([])
    expect(hasAnyUsageData(EMPTY)).toBe(false)
  })

  it('counts per-agent lines as data, so the card cannot deny what it displays', () => {
    // A provider can report per-agent figures without session totals. When
    // this checked only the rows, the card printed "No usage data yet for
    // this chat" directly above a populated per-agent list.
    const agentsOnly: SessionUsage = {
      ...EMPTY,
      // A helper, not a lone lead: a lone lead IS the session total, so it
      // deliberately produces no breakdown (see `buildAgentUsageLines`).
      agents: [
        { executionId: 'lead', label: 'Lead agent', isLead: true, tokens: 1200, inputTokens: null, outputTokens: null, turns: 2, costMicroUsd: null },
        { executionId: 'h1', label: 'Fix the parser', isLead: false, tokens: 400, inputTokens: null, outputTokens: null, turns: 1, costMicroUsd: null },
      ],
    }
    expect(buildUsageRows(agentsOnly)).toEqual([])
    expect(buildAgentUsageLines(agentsOnly).length).toBeGreaterThan(0)
    expect(hasAnyUsageData(agentsOnly)).toBe(true)
  })

  it('never shows a zero for a field that was never measured', () => {
    // Only activeHelperCount is present (as a real, measured zero) -- every
    // other field stays absent and must not appear as "0" rows.
    const usage: SessionUsage = { ...EMPTY, activeHelperCount: 0 }
    const rows = buildUsageRows(usage)
    expect(rows).toHaveLength(1)
    expect(rows[0].key).toBe('helpers')
    expect(rows[0].value).toBe('0')
  })

  it('combines session tokens and requests into one row when both are present', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionTokens: { value: 31000, source: 'measured' },
      sessionRequests: { value: 7, source: 'measured' },
    }
    const row = buildUsageRows(usage).find((r) => r.key === 'session')!
    expect(row.label).toBe('Current session')
    expect(row.value).toContain('31k tokens')
    expect(row.value).toContain('7 turns')
    expect(row.isEstimate).toBe(false)
  })

  it('shows tokens alone when requests are not measured', () => {
    const usage: SessionUsage = { ...EMPTY, sessionTokens: { value: 1200, source: 'measured' } }
    const row = buildUsageRows(usage).find((r) => r.key === 'session')!
    expect(row.value).toBe('1.2k tokens')
  })

  it('marks a row as an estimate when its source is estimated', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.42, source: 'estimated' },
    }
    const row = buildUsageRows(usage).find((r) => r.key === 'cost')!
    expect(row.isEstimate).toBe(true)
    expect(row.value).toBe('$0.42')
  })

  it('does not mark a measured or provider-reported row as an estimate', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      planLimit: { value: 2409, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].isEstimate).toBe(false)
  })

  it('calls a provider-reported cost a cost, not an estimate', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.42, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Cost')
    expect(rows[0].isEstimate).toBe(false)
  })

  it('shows a sub-cent cost instead of rounding it away to $0.00', () => {
    // A single turn routinely costs well under a cent. Two decimal places
    // would render a real, provider-reported figure as free.
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.0034, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('$0.0034')
  })

  it('formats big numbers the same way the usage card does', () => {
    // Labelled "in" because output was never reported: this is not a total.
    expect(nodeUsageLine({ inputTokens: 1_500_000 })).toBe('1.5m tokens in')
    expect(nodeUsageLine({ inputTokens: 1_000_000, outputTokens: 500_000 })).toBe('1.5m tokens')
  })

  it('never rounds a real charge down to nothing', () => {
    // Cost arrives in millionths of a dollar, so four decimal places is not
    // enough on its own -- anything under $0.00005 would print "$0.0000",
    // which is the same "a real charge shown as free" bug in a new place.
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0.000004, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('< $0.0001')
  })

  it('shows a genuine zero cost as $0.00 rather than padding it', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionCostUsd: { value: 0, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)[0].value).toBe('$0.00')
  })

  it('reports session tokens the provider counted', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionTokens: { value: 31_400, source: 'providerReported' },
      sessionRequests: { value: 7, source: 'measured' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Current session')
    expect(rows[0].value).toBe('31k tokens · 7 turns')
    expect(rows[0].isEstimate).toBe(false)
  })

  it('shows context as a share of the window rather than raw numbers', () => {
    // "31k of 200k" makes the reader do the division to answer the only
    // question they have, which is how close a compaction is.
    const usage: SessionUsage = {
      ...EMPTY,
      contextUsed: { value: 31_000, source: 'providerReported' },
      contextSize: { value: 200_000, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage)
    expect(rows[0].label).toBe('Context used')
    expect(rows[0].value).toBe('16% of 200k tokens')
  })

  it('shows no context row when only half of it was reported', () => {
    // A window size with no occupancy, or the reverse, says nothing useful.
    expect(
      buildUsageRows({ ...EMPTY, contextSize: { value: 200_000, source: 'providerReported' } })
    ).toEqual([])
  })

  it('does not divide by a zero window', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      contextUsed: { value: 10, source: 'providerReported' },
      contextSize: { value: 0, source: 'providerReported' },
    }
    expect(buildUsageRows(usage)).toEqual([])
  })

  /**
   * No AI tool GitWyrm talks to reports a plan allowance today, so the row
   * used to be omitted entirely -- answering "how much have I got left?"
   * with silence, which reads as "nothing to say" rather than "GitWyrm
   * cannot see this".
   */
  it('says the allowance is not reported rather than leaving it out', () => {
    const usage: SessionUsage = { ...EMPTY, sessionTokens: { value: 1200, source: 'measured' } }
    const row = buildUsageRows(usage).find((r) => r.key === 'planLimit')!
    expect(row.value).toBe('not reported')
    // Not an estimate: GitWyrm is not guessing, it simply was not told.
    expect(row.isEstimate).toBe(false)
  })

  it('shows the real allowance instead of the not-reported line when one arrives', () => {
    const usage: SessionUsage = {
      ...EMPTY,
      sessionTokens: { value: 1200, source: 'measured' },
      planLimit: { value: 2409, source: 'providerReported' },
    }
    const rows = buildUsageRows(usage).filter((r) => r.key === 'planLimit')
    expect(rows).toHaveLength(1)
    expect(rows[0].value).toBe('2,409')
  })

  /**
   * A chat whose only row is "0 helpers active" has not run yet. Answering
   * a question nobody has asked there is clutter, and the card's own empty
   * state already covers a chat with nothing to show.
   */
  it('stays quiet about the allowance on a chat with no figures yet', () => {
    const usage: SessionUsage = { ...EMPTY, activeHelperCount: 0 }
    expect(buildUsageRows(usage).some((r) => r.key === 'planLimit')).toBe(false)
  })

  it('formats the reset date without inventing a time-of-day claim', () => {
    const usage: SessionUsage = { ...EMPTY, planResetAt: '2026-09-01T00:00:00Z' }
    const row = buildUsageRows(usage).find((r) => r.key === 'resets')!
    expect(row.label).toBe('Resets')
  })
})

describe('nodeUsageLine', () => {
  it('says what one agent spent', () => {
    const line = nodeUsageLine({ inputTokens: 1200, outputTokens: 800, turns: 3, costMicroUsd: 40000 })
    expect(line).toMatch(/2.0k tokens/)
    // Same formatter as the usage card, so the two can never disagree about
    // the same number -- a local copy once printed "1.5M" where the card
    // printed "1.5m", and only above a million, where it mattered most.
    expect(line).toMatch(/3 turns/)
    expect(line).toMatch(/\$0.04/)
  })

  it('does not count a half it was never told', () => {
    // Each token field is looked up independently on the Rust side, so a
    // provider can report one and not the other. Treating the missing half as
    // zero would present a partial figure as the node's whole spend.
    expect(nodeUsageLine({ inputTokens: 1200 })).toBe('1.2k tokens in')
    expect(nodeUsageLine({ outputTokens: 800 })).toBe('800 tokens out')
    expect(nodeUsageLine({ inputTokens: 1200, outputTokens: 800 })).toBe('2.0k tokens')
  })

  it('uses the singular for one turn', () => {
    expect(nodeUsageLine({ turns: 1 })).toBe('1 turn')
  })

  it('says nothing at all when the provider reported nothing', () => {
    // The rule every usage surface follows: absent is not zero. A row of
    // zeros would read as "measured, and free".
    expect(nodeUsageLine(null)).toBeNull()
    expect(nodeUsageLine(undefined)).toBeNull()
    expect(nodeUsageLine({})).toBeNull()
  })

  it('leaves out the parts that were not reported', () => {
    expect(nodeUsageLine({ turns: 2 })).toBe('2 turns')
    // This asserted a bare '500 tokens', which contradicted the test's own
    // name: output was not reported, so the figure is not the node's total.
    expect(nodeUsageLine({ inputTokens: 500 })).toBe('500 tokens in')
  })

  it('never rounds a real charge down to nothing', () => {
    expect(nodeUsageLine({ costMicroUsd: 20 })).toBe('< $0.0001')
  })
})

describe('explainUsageUnavailable', () => {
  it('says a damaged file means the cost is unknown, not zero', () => {
    const msg = explainUsageUnavailable({ kind: 'damaged', reason: 'bad json' }, false)
    expect(msg).toMatch(/unknown/i)
    expect(msg).toMatch(/bad json/)
  })
  it('passes through why it is unavailable right now', () => {
    expect(explainUsageUnavailable({ kind: 'unavailable', detail: 'file is locked' }, false)).toMatch(/file is locked/)
  })
  it('treats a transport failure as unknown too', () => {
    expect(explainUsageUnavailable(undefined, true)).toMatch(/could not read/i)
  })
  it('stays silent for the genuinely empty case', () => {
    // "Nothing recorded" is a real answer, and the plain empty state says it
    // better than an error would.
    expect(explainUsageUnavailable({ kind: 'notFound' }, false)).toBeNull()
    expect(explainUsageUnavailable(undefined, false)).toBeNull()
  })
})

describe('buildAgentUsageLines with a half-known token figure', () => {
  const agent = (over: Record<string, unknown>) => ({
    executionId: 'e',
    label: 'Helper',
    isLead: false,
    tokens: null,
    inputTokens: null,
    outputTokens: null,
    costMicroUsd: null,
    turns: null,
    ...over,
  })
  // Two agents, or `buildAgentUsageLines` deliberately produces no breakdown.
  const withAgents = (helper: ReturnType<typeof agent>) =>
    ({ agents: [agent({ executionId: 'lead', inputTokens: null, outputTokens: null, isLead: true }), helper] }) as never

  it('names the half it knows rather than showing it as a total', () => {
    const lines = buildAgentUsageLines(withAgents(agent({ inputTokens: 500 })))
    expect(lines[1].parts.join(' ')).toMatch(/500 tokens in/)
  })
  it('still shows a real total when both halves are known', () => {
    const lines = buildAgentUsageLines(withAgents(agent({ tokens: 1200, inputTokens: 500, outputTokens: 700 })))
    expect(lines[1].parts.join(' ')).toMatch(/1\.2k tokens/)
    expect(lines[1].parts.join(' ')).not.toMatch(/ in| out/)
  })
})

describe('the graph and the cost card agree about a reported zero', () => {
  // Both read the same `usage.turns` off the same execution record
  // (`agent_desk.rs` builds the card's row as `turns: Some(usage.turns)`),
  // so the same helper is described by both surfaces at once. If they
  // disagree about what zero means, the person sees a helper credited with
  // "0 turns" in one panel and no turn count at all in the other.
  const zeroTurns = { inputTokens: 100, outputTokens: 50, cachedInputTokens: null, costMicroUsd: null, turns: 0 }

  it('shows the same turn count in both places', () => {
    const graph = nodeUsageLine(zeroTurns)
    const card = buildAgentUsageLines({
      sessionTokens: null,
      sessionRequests: null,
      sessionCostUsd: null,
      planLimit: null,
      planResetAt: null,
      contextUsed: null,
      contextSize: null,
      activeHelperCount: null,
      dataTimestamp: '2026-09-04T00:00:00Z',
      agents: [
        { executionId: 'lead', label: 'Lead agent', isLead: true, tokens: 900, inputTokens: null, outputTokens: null, turns: 3, costMicroUsd: null },
        { executionId: 'h1', label: 'Helper', isLead: false, tokens: 150, inputTokens: null, outputTokens: null, turns: 0, costMicroUsd: null },
      ],
    })
    const helperLine = card.find((l) => l.key === 'h1')!.parts.join(' · ')
    expect(helperLine.includes('turn')).toBe(graph!.includes('turn'))
  })
})

describe('a turn count GitWyrm measured itself is not "not reported"', () => {
  // The backend has a helper (`provider_reported_nothing`) that answers a
  // NEARBY question: did the provider report any figures? For a run with
  // turns but no tokens the answer is yes-nothing-reported -- and it was
  // once named `is_unreported`, with a doc saying the UI should render that
  // case as "not reported".
  //
  // That would be wrong. `turns` is counted by GitWyrm, not asked of the
  // provider, so it is measured. Hiding it discards a real number, which is
  // the "unknown stays unknown, never zero" rule inverted. This pins the
  // behaviour so wiring that helper up later cannot quietly change it.
  it('still shows the turns when no token figures came back', () => {
    const lines = buildAgentUsageLines({
      sessionTokens: null,
      sessionRequests: null,
      sessionCostUsd: null,
      planLimit: null,
      planResetAt: null,
      contextUsed: null,
      contextSize: null,
      activeHelperCount: null,
      dataTimestamp: '2026-09-04T00:00:00Z',
      agents: [
        { executionId: 'lead', label: 'Lead agent', isLead: true, tokens: 900, inputTokens: null, outputTokens: null, turns: 2, costMicroUsd: null },
        { executionId: 'h1', label: 'Helper', isLead: false, tokens: null, inputTokens: null, outputTokens: null, turns: 3, costMicroUsd: null },
      ],
    })
    const helper = lines.find((l) => l.key === 'h1')!
    expect(helper.parts.join(' · ')).toContain('3 turns')
    expect(helper.parts.length).toBeGreaterThan(0)
  })
})

describe('cached input tokens', () => {
  // Both provider paths report this and nothing rendered it -- a measured
  // number thrown away, which is the mirror of inventing an absent one.
  it('is named beside the total, never added into it', () => {
    const line = nodeUsageLine({
      inputTokens: 1000,
      outputTokens: 200,
      cachedInputTokens: 800,
      costMicroUsd: null,
      turns: 2,
    })!
    // 1000 + 200, NOT 1000 + 200 + 800: cached input is a subset of input.
    expect(line).toContain('1.2k tokens')
    expect(line).toContain('800 tokens cached')
  })

  it('says nothing when there is no input figure to be a fraction of', () => {
    const line = nodeUsageLine({
      inputTokens: null,
      outputTokens: 200,
      cachedInputTokens: 800,
      costMicroUsd: null,
      turns: 1,
    })!
    expect(line).not.toContain('cached')
  })

  it('is absent, not zero, when the provider did not report it', () => {
    const line = nodeUsageLine({
      inputTokens: 1000,
      outputTokens: 200,
      cachedInputTokens: null,
      costMicroUsd: null,
      turns: 1,
    })!
    expect(line).not.toContain('cached')
  })
})

describe('a measured figure is not marked as an estimate', () => {
  // `UsageSource::Measured` had never been constructed by the backend, so the
  // frontend had never actually received one. Now that turn counts carry it
  // (GitWyrm counts them itself), check the card treats it as a real figure
  // rather than falling through to the "est." badge.
  it('shows no estimate badge for a measured turn count', () => {
    const rows = buildUsageRows({
      sessionTokens: null,
      sessionRequests: { value: 5, source: 'measured' },
      sessionCostUsd: null,
      planLimit: null,
      planResetAt: null,
      contextUsed: null,
      contextSize: null,
      activeHelperCount: null,
      dataTimestamp: '2026-09-04T00:00:00Z',
      agents: [],
    })
    const session = rows.find((r) => r.key === 'session')!
    expect(session.value).toContain('5 turns')
    expect(session.isEstimate).toBe(false)
  })
})
