import { useRef, useState } from 'react'
import { cn } from '@/lib/utils'

interface ResizeHandleProps {
  ariaLabel: string
  value?: number
  min: number
  max: number
  defaultValue: number
  onChange: (value: number) => void
  onReset?: () => void
  /** Use -1 when dragging left should increase the controlled width. */
  direction?: 1 | -1
  getCurrentValue?: (handle: HTMLDivElement) => number
  className?: string
}

function clamp(value: number, min: number, max: number): number {
  return Math.round(Math.min(max, Math.max(min, value)))
}

/**
 * Keyboard-accessible vertical resize handle shared by workspace panes and
 * graph columns. Dragging updates live, arrow keys move by 8px, and a double
 * click restores the default width.
 */
export function ResizeHandle({
  ariaLabel,
  value,
  min,
  max,
  defaultValue,
  onChange,
  onReset,
  direction = 1,
  getCurrentValue,
  className,
}: ResizeHandleProps) {
  const dragStart = useRef<{ pointerId: number; x: number; value: number } | null>(null)
  const [resizing, setResizing] = useState(false)

  const currentValue = (handle: HTMLDivElement) =>
    clamp(value ?? getCurrentValue?.(handle) ?? defaultValue, min, max)

  const finishResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const start = dragStart.current
    if (!start || start.pointerId !== event.pointerId) return
    dragStart.current = null
    setResizing(false)
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
  }

  return (
    <div
      role="separator"
      aria-label={ariaLabel}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={clamp(value ?? defaultValue, min, max)}
      tabIndex={0}
      draggable={false}
      onPointerDown={(event) => {
        event.preventDefault()
        event.stopPropagation()
        dragStart.current = {
          pointerId: event.pointerId,
          x: event.clientX,
          value: currentValue(event.currentTarget),
        }
        event.currentTarget.setPointerCapture(event.pointerId)
        setResizing(true)
      }}
      onPointerMove={(event) => {
        const start = dragStart.current
        if (!start || start.pointerId !== event.pointerId) return
        onChange(clamp(start.value + (event.clientX - start.x) * direction, min, max))
      }}
      onPointerUp={finishResize}
      onPointerCancel={(event) => {
        if (dragStart.current?.pointerId !== event.pointerId) return
        dragStart.current = null
        setResizing(false)
      }}
      onDoubleClick={(event) => {
        event.preventDefault()
        event.stopPropagation()
        if (onReset) onReset()
        else onChange(defaultValue)
      }}
      onKeyDown={(event) => {
        if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
        event.preventDefault()
        event.stopPropagation()
        const delta = event.key === 'ArrowLeft' ? -8 : 8
        onChange(clamp(currentValue(event.currentTarget) + delta * direction, min, max))
      }}
      onClick={(event) => event.stopPropagation()}
      className={cn(
        'group absolute bottom-0 top-0 z-30 w-2 cursor-col-resize touch-none select-none outline-none',
        'after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-transparent after:transition-colors',
        'hover:after:bg-primary focus-visible:after:bg-primary',
        resizing && 'after:bg-primary',
        className,
      )}
    />
  )
}
