import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { commands, type OpenResultDiffTarget } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import { useUiStore } from '@/stores/uiStore'
import { log, describeError } from '@/lib/log'

const OPEN_RESULT_DIFF_EVENT = 'agent-result://open-diff'

// The payload type is generated (`lib.rs` registers it with `.typ::<>()`,
// which is how an event payload reaches the exporter -- it is never a
// command parameter or return, so nothing else would reach it). This file
// used to hand-write its own copy, which meant renaming the field in Rust
// compiled and typechecked cleanly while breaking this listener at runtime.

/**
 * Main-window-only listener for `agent-result://open-diff`
 * (`commands::agent_result::agent_result_open_diff`, tasks.md 2.2).
 *
 * This is the bridge `src/lib/agentDeskTargets.ts` was left waiting for: the
 * Agent Desk window has no embedded diff viewer, so "View diff" on a result
 * asks the backend to focus THIS window and emit the worktree/path here.
 * `DiffView` (`src/views/DiffView.tsx`) only ever renders a diff for the
 * currently active repo TAB (`useActiveRepo`) -- a helper's isolated
 * worktree is a different path than any tab the user has open, so this
 * opens (or focuses, `addRepo` already dedupes by path) that worktree as an
 * ordinary repo tab before pointing `uiStore.openDiff` at it. No new diff
 * renderer, no diff text crossing the IPC boundary a second way.
 *
 * Register this once, only where a main-window instance of `App.tsx`
 * mounts (`AppInner`, gated on `mode.kind` being neither `spec-desk` nor
 * `agent-desk`) -- Agent Desk's own window must never try to open a repo
 * tab, it has none.
 */
export function useAgentResultDiffListener() {
  const addRepo = useWorkspaceStore((s) => s.addRepo)
  const setActiveRepo = useWorkspaceStore((s) => s.setActiveRepo)
  const openDiff = useUiStore((s) => s.openDiff)

  useEffect(() => {
    const unlisten = listen<OpenResultDiffTarget>(OPEN_RESULT_DIFF_EVENT, (event) => {
      void (async () => {
        const { worktreePath, path } = event.payload
        const result = await commands.openRepo(worktreePath)
        if (result.status === 'error') {
          toast.error(`Could not open that worktree: ${result.error}`)
          log.warn(`agent result diff: openRepo failed for ${worktreePath}: ${result.error}`)
          return
        }
        const repo = unwrap(result)
        addRepo(repo)
        setActiveRepo(repo.id)
        if (path) {
          openDiff({ path, source: { kind: 'unstaged' } })
        }
      })().catch((e: unknown) => {
        log.error(`agent result diff listener failed: ${describeError(e)}`)
      })
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [addRepo, setActiveRepo, openDiff])
}
