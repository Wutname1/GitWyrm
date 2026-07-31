import {
  CheckIcon,
  CircleCheckIcon,
  CopyIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { useEffect } from "react"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import { log } from "@/lib/log"

const COPY_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>'

const CHECK_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>'

/**
 * Add a copy button to every toast as it appears.
 *
 * Sonner has no slot for extra content that applies to all toasts: `cancel` and
 * `action` are per-toast only, and toasts are raised from ~47 call sites that
 * pass just a message string. Rather than change all of them, watch the toaster
 * and inject the button once per toast. The text copied is read from the
 * toast's own `[data-content]`, so it is exactly what the user is reading.
 */
function useToastCopyButton() {
  useEffect(() => {
    async function handleCopy(this: HTMLButtonElement) {
      const text = this.closest("[data-sonner-toast]")
        ?.querySelector("[data-content]")
        ?.textContent?.trim()
      if (!text) return

      try {
        if (!navigator.clipboard) throw new Error("clipboard unavailable")
        await navigator.clipboard.writeText(text)
        this.innerHTML = CHECK_ICON
        this.title = "Copied"
        window.setTimeout(() => {
          this.innerHTML = COPY_ICON
          this.title = "Copy message"
        }, 1500)
      } catch (e) {
        log.warn(`toast copy failed: ${(e as Error).message}`)
      }
    }

    function addCopyButtons() {
      for (const toastEl of document.querySelectorAll("[data-sonner-toast]")) {
        if (toastEl.querySelector(".gw-toast-copy")) continue
        // Loading toasts are transient status, not a message worth copying.
        if (toastEl.getAttribute("data-type") === "loading") continue

        const button = document.createElement("button")
        button.type = "button"
        button.className = "gw-toast-copy"
        button.title = "Copy message"
        button.setAttribute("aria-label", "Copy message")
        button.innerHTML = COPY_ICON
        button.addEventListener("click", handleCopy)
        toastEl.appendChild(button)
      }
    }

    addCopyButtons()
    const observer = new MutationObserver(addCopyButtons)
    observer.observe(document.body, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [])
}

const Toaster = ({ ...props }: ToasterProps) => {
  useToastCopyButton()

  return (
    <Sonner
      theme="dark"
      className="toaster group"
      closeButton
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
