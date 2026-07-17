import { cn } from '@/lib/utils'
import type { RefInfo, RefKind } from '@/lib/bindings'
import { RefContextMenu } from './RefContextMenu'

const styles: Record<RefKind, string> = {
  head: 'bg-primary text-primary-foreground',
  branch: 'border border-primary text-primary',
  remote: 'border border-border text-sub',
  tag: 'bg-modified text-[#1a1400]',
}

export function RefBadge({ refTag }: { refTag: RefInfo }) {
  return (
    <RefContextMenu refTag={refTag}>
      <span
        className={cn(
          'max-w-[92px] flex-none cursor-default overflow-hidden text-ellipsis whitespace-nowrap rounded-[5px] px-1.5 py-px font-mono text-[9.5px] font-semibold leading-[1.4]',
          styles[refTag.type]
        )}
      >
        {refTag.name}
      </span>
    </RefContextMenu>
  )
}
