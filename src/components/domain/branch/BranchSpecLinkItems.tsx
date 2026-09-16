import { Check, Link2, Unlink } from 'lucide-react'
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@/components/ui/context-menu'
import { useOpenspecChanges, useOpenspecStatus } from '@/hooks/useOpenspec'
import { useSpecLink, useSpecLinkMutations } from '@/hooks/useSpecLink'

interface BranchSpecLinkItemsProps {
  branch: string
  repoId: string | null
}

/**
 * Menu items for saying which change a branch is working on.
 *
 * Renders nothing at all in a repo without an `openspec/` folder -- the same
 * rule every other Specs surface follows.
 */
export function BranchSpecLinkItems({ branch, repoId }: BranchSpecLinkItemsProps) {
  const status = useOpenspecStatus(repoId)
  const changes = useOpenspecChanges(repoId)
  const current = useSpecLink(repoId, branch)
  const { link, unlink } = useSpecLinkMutations(repoId)

  if (!status.data?.present) return null

  const linked = current.data
  const options = changes.data ?? []

  return (
    <>
      <ContextMenuSeparator />
      <ContextMenuSub>
        <ContextMenuSubTrigger>
          <Link2 />
          {linked ? `Working on ${linked.change_id}` : 'Say what this branch works on'}
        </ContextMenuSubTrigger>
        <ContextMenuSubContent className="max-h-80 w-72 overflow-y-auto">
          {/* Three different facts, and one sentence used to cover all of
              them: the scan has not finished, the scan failed, and there
              genuinely are none. Saying "no changes to pick from yet" while
              GitWyrm is still looking, or could not look at all, tells
              somebody their specs are missing when they are not. */}
          {options.length === 0 &&
            (changes.isPending ? (
              <ContextMenuItem disabled>Looking for changes...</ContextMenuItem>
            ) : changes.isError ? (
              <ContextMenuItem disabled>GitWyrm could not read this project's changes.</ContextMenuItem>
            ) : (
              <ContextMenuItem disabled>No changes to pick from yet</ContextMenuItem>
            ))}
          {options.map((change) => {
            const active = linked?.change_id === change.id
            return (
              <ContextMenuItem
                key={change.id}
                onSelect={() => {
                  if (!active) link.mutate({ branch, changeId: change.id })
                }}
              >
                <div className="flex min-w-0 flex-col">
                  <span className="truncate">{change.title}</span>
                  <span className="truncate font-mono text-2xs text-muted-foreground">
                    {change.id}
                    {change.progress.total > 0 &&
                      ` · ${change.progress.done}/${change.progress.total} done`}
                  </span>
                </div>
                {active && <Check size={13} className="ml-auto shrink-0 text-accent-text" />}
              </ContextMenuItem>
            )
          })}
          {linked?.explicit && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={() => unlink.mutate({ branch })}>
                <Unlink />
                Stop tying this branch to a change
              </ContextMenuItem>
            </>
          )}
        </ContextMenuSubContent>
      </ContextMenuSub>
    </>
  )
}
