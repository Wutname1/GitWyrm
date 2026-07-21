import { statusColor } from '@/lib/gitDisplay'
import type { StatusCode } from '@/lib/bindings'

export function StatusBadge({ st }: { st: StatusCode }) {
  const color = statusColor(st)
  return (
    <span
      className="flex size-4 flex-none items-center justify-center rounded-[3px] border font-mono text-2xs font-bold"
      style={{ color, borderColor: color }}
    >
      {st}
    </span>
  )
}
