import { useState } from 'react'
import { useAgentSession, useAgentSessionHeaders } from '@/hooks/useAgentSessions'
import type { AgentSessionHeader } from '@/lib/bindings'

/**
 * Temporary developer surface for Agent Desk sessions.
 *
 * Task 6.2 of `openspec/changes/agent-desk-session-foundation/tasks.md` asks
 * for something that proves the list/read data layer end-to-end -- headers
 * loading, a session opening, live events landing -- without final styling.
 * The real `AgentDeskView` from `docs/agent-desk/architecture.md` section 7
 * replaces this in a later change.
 */
export function AgentDeskDevView() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const { headers, diagnostics, isLoading, hasNextPage, fetchNextPage } =
    useAgentSessionHeaders()
  const { session, messages, state, isLoading: sessionLoading } = useAgentSession(selectedId)

  return (
    <div style={{ display: 'flex', height: '100%', fontFamily: 'monospace', fontSize: 12 }}>
      <div style={{ width: 280, overflowY: 'auto', borderRight: '1px solid #444', padding: 8 }}>
        <p style={{ fontWeight: 'bold' }}>Agent Desk sessions (dev)</p>
        {isLoading && <p>Loading...</p>}
        {diagnostics.length > 0 && (
          <p style={{ color: 'orange' }}>{diagnostics.length} file(s) could not be read</p>
        )}
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {headers.map((header: AgentSessionHeader) => (
            <li key={header.sessionId}>
              <button
                type="button"
                onClick={() => setSelectedId(header.sessionId)}
                style={{
                  display: 'block',
                  width: '100%',
                  textAlign: 'left',
                  padding: '4px 6px',
                  background:
                    header.sessionId === selectedId ? 'rgba(128,128,255,0.2)' : 'transparent',
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                {header.unread ? '● ' : ''}
                {header.title || '(untitled)'} - {header.state}
              </button>
            </li>
          ))}
        </ul>
        {hasNextPage && (
          <button type="button" onClick={() => fetchNextPage()}>
            Load more
          </button>
        )}
      </div>
      <div style={{ flex: 1, overflowY: 'auto', padding: 8 }}>
        {!selectedId && <p>Select a session.</p>}
        {selectedId && sessionLoading && <p>Loading session...</p>}
        {selectedId && !sessionLoading && !session && <p>Session not found or damaged.</p>}
        {session && (
          <>
            <p style={{ fontWeight: 'bold' }}>
              {session.header.title || '(untitled)'} - state: {state}
            </p>
            <p>source: {session.header.source.kind}</p>
            <ul style={{ listStyle: 'none', padding: 0 }}>
              {messages.map((m) => (
                <li key={m.messageId} style={{ marginBottom: 6 }}>
                  <strong>{m.role}</strong> ({m.kind}): {m.plainContent}
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </div>
  )
}
