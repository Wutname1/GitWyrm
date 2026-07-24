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
  /**
   * Use -1 when dragging left/up should increase the controlled size (a handle
   * on an element's leading edge).
   */
  direction?: 1 | -1
  /** 'x' resizes a width (default); 'y' resizes a height. */
  axis?: 'x' | 'y'
  getCurrentValue?: (handle: HTMLDivElement) => number
  className?: string
}

function clamp(value: number, min: number, max: number): number {
  return Math.round(Math.min(max, Math.max(min, value)))
}

/**
 * Keyboard-accessible resize handle shared by workspace panes, the commit
 * drawer, and graph columns. Dragging updates live, arrow keys move by 8px,
 * and a double click restores the default size.
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
  axis = 'x',
  getCurrentValue,
  className,
}: ResizeHandleProps) {
  const dragStart = useRef<{ pointerId: number; x: number; y: number; value: number } | null>(null)
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

  const growKey = axis === 'y' ? 'ArrowDown' : 'ArrowRight'
  const shrinkKey = axis === 'y' ? 'ArrowUp' : 'ArrowLeft'

  return (
    <div
      role="separator"
      aria-label={ariaLabel}
      aria-orientation={axis === 'y' ? 'horizontal' : 'vertical'}
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
          y: event.clientY,
          value: currentValue(event.currentTarget),
        }
        event.currentTarget.setPointerCapture(event.pointerId)
        setResizing(true)
      }}
      onPointerMove={(event) => {
        const start = dragStart.current
        if (!start || start.pointerId !== event.pointerId) return
        const delta = axis === 'y' ? event.clientY - start.y : event.clientX - start.x
        onChange(clamp(start.value + delta * direction, min, max))
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
        if (event.key !== growKey && event.key !== shrinkKey) return
        event.preventDefault()
        event.stopPropagation()
        const delta = event.key === shrinkKey ? -8 : 8
        onChange(clamp(currentValue(event.currentTarget) + delta * direction, min, max))
      }}
      onClick={(event) => event.stopPropagation()}
      className={cn(
        'group absolute z-30 touch-none select-none outline-none',
        axis === 'y'
          ? 'inset-x-0 h-2 cursor-row-resize after:absolute after:inset-x-0 after:top-1/2 after:h-px after:-translate-y-1/2'
          : 'bottom-0 top-0 w-2 cursor-col-resize after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2',
        'after:bg-transparent after:transition-colors',
        'hover:after:bg-primary focus-visible:after:bg-primary',
        resizing && 'after:bg-primary',
        className,
      )}
    />
  )
}
