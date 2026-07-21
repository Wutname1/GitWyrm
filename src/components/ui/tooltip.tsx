"use client"

import * as React from "react"
import { Tooltip as TooltipPrimitive } from "radix-ui"

import { cn } from "@/lib/utils"

function TooltipProvider({
  delayDuration = 0,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return (
    <TooltipPrimitive.Provider
      data-slot="tooltip-provider"
      delayDuration={delayDuration}
      {...props}
    />
  )
}

function Tooltip({
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Root>) {
  return <TooltipPrimitive.Root data-slot="tooltip" {...props} />
}

function TooltipTrigger({
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Trigger>) {
  return <TooltipPrimitive.Trigger data-slot="tooltip-trigger" {...props} />
}

function TooltipContent({
  className,
  sideOffset = 7,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        data-slot="tooltip-content"
        sideOffset={sideOffset}
        className={cn(
          "z-[100] w-fit max-w-72 origin-(--radix-tooltip-content-transform-origin) animate-in rounded-[5px] border border-border bg-panel3 px-2.5 py-1.5 font-sans text-2xs leading-4 text-balance text-foreground shadow-[0_8px_24px_rgba(0,0,0,0.42),inset_0_1px_0_rgba(255,255,255,0.04)] fade-in-0 zoom-in-95 data-[side=bottom]:slide-in-from-top-1 data-[side=left]:slide-in-from-right-1 data-[side=right]:slide-in-from-left-1 data-[side=top]:slide-in-from-bottom-1 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 motion-reduce:animate-none",
          className
        )}
        {...props}
      >
        {children}
        <TooltipPrimitive.Arrow className="z-[100] size-2.5 translate-y-[calc(-50%_-_2px)] rotate-45 rounded-[2px] bg-panel3 fill-panel3 stroke-border stroke-1" />
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  )
}

type TooltipButtonProps = Omit<React.ComponentProps<"button">, "title"> & {
  tooltip: React.ReactNode
  tooltipSide?: React.ComponentProps<typeof TooltipPrimitive.Content>["side"]
}

function TooltipButton({ tooltip, tooltipSide, "aria-label": ariaLabel, ...props }: TooltipButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={ariaLabel ?? (typeof tooltip === "string" ? tooltip : undefined)}
          {...props}
        />
      </TooltipTrigger>
      <TooltipContent side={tooltipSide}>{tooltip}</TooltipContent>
    </Tooltip>
  )
}

type TooltipHintProps = {
  children: React.ReactElement
  label: React.ReactNode
  side?: React.ComponentProps<typeof TooltipPrimitive.Content>["side"]
}

function TooltipHint({ children, label, side }: TooltipHintProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side={side}>{label}</TooltipContent>
    </Tooltip>
  )
}

export {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
  TooltipButton,
  TooltipHint,
}
