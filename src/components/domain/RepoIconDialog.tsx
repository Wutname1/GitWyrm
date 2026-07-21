import { useEffect, useState } from 'react'
import { FolderOpen, ImageIcon, Loader2, RotateCcw } from 'lucide-react'
import { toast } from 'sonner'
import { commands, type RepoIcon, type RepoInfo } from '@/lib/bindings'
import { unwrap } from '@/lib/queryKeys'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface RepoIconDialogProps {
  repo: RepoInfo
  open: boolean
  onOpenChange: (open: boolean) => void
  onIconChanged: () => void
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

export function RepoIconDialog({ repo, open, onOpenChange, onIconChanged }: RepoIconDialogProps) {
  const [icons, setIcons] = useState<RepoIcon[]>([])
  const [current, setCurrent] = useState<RepoIcon | null>(null)
  const [loading, setLoading] = useState(false)
  const [savingPath, setSavingPath] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    let active = true
    setLoading(true)
    Promise.all([
      commands.findRepoIcons(repo.path).then(unwrap),
      commands.getRepoIcon(repo.path).then(unwrap),
    ])
      .then(([found, selected]) => {
        if (!active) return
        setIcons(found)
        setCurrent(selected)
      })
      .catch((error: unknown) => {
        if (active) toast.error(`Could not look for icons: ${messageFrom(error)}`)
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => { active = false }
  }, [open, repo.path])

  const saveIcon = async (sourcePath: string) => {
    setSavingPath(sourcePath)
    try {
      const saved = unwrap(await commands.setRepoIcon(repo.path, sourcePath))
      setCurrent(saved)
      onIconChanged()
      onOpenChange(false)
      toast.success(`${repo.name} now has a custom icon`)
    } catch (error: unknown) {
      toast.error(`Could not set the icon: ${messageFrom(error)}`)
    } finally {
      setSavingPath(null)
    }
  }

  const chooseFile = async () => {
    const { open: openDialog } = await import('@tauri-apps/plugin-dialog')
    const selected = await openDialog({
      title: `Choose an icon for ${repo.name}`,
      multiple: false,
      directory: false,
      filters: [{
        name: 'Images',
        extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'ico', 'svg'],
      }],
    })
    if (typeof selected === 'string') await saveIcon(selected)
  }

  const useAutomaticIcon = async () => {
    setSavingPath('automatic')
    try {
      const found = unwrap(await commands.clearRepoIcon(repo.path))
      setCurrent(found)
      onIconChanged()
      onOpenChange(false)
      toast.success(found
        ? `${repo.name} is using the icon found in its files`
        : `${repo.name} is using the default tab marker`)
    } catch (error: unknown) {
      toast.error(`Could not reset the icon: ${messageFrom(error)}`)
    } finally {
      setSavingPath(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="overflow-hidden p-0 sm:max-w-md">
        <DialogHeader className="border-b border-border px-5 py-4 pr-12">
          <div className="flex items-center gap-3">
            <div className="grid size-9 flex-none place-items-center rounded-lg border border-border bg-panel2 text-accent-text">
              {current
                ? <img src={current.data_url} alt="" className="size-6 rounded object-cover" />
                : <ImageIcon size={17} strokeWidth={1.8} />}
            </div>
            <div className="min-w-0">
              <DialogTitle className="truncate text-sm">Set icon for {repo.name}</DialogTitle>
              <DialogDescription className="mt-1 text-xs">
                Pick one GitWyrm found, or choose an image from your computer.
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="grid gap-3 px-5 py-4">
          <div className="flex items-center justify-between gap-3">
            <span className="text-2xs font-semibold uppercase tracking-wide text-sub">
              Found in this repository
            </span>
            {loading && (
              <span className="flex items-center gap-1.5 text-2xs text-muted-foreground">
                <Loader2 size={11} className="animate-spin" />
                Looking
              </span>
            )}
          </div>

          {!loading && icons.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border bg-panel/50 px-4 py-5 text-center">
              <ImageIcon size={20} className="mx-auto mb-2 text-muted-foreground" strokeWidth={1.6} />
              <p className="text-xs font-medium text-foreground">No icon files found</p>
              <p className="mt-1 text-2xs leading-relaxed text-muted-foreground">
                GitWyrm looked for common favicon, logo, and app icon names.
              </p>
            </div>
          ) : (
            <div className="grid max-h-52 grid-cols-4 gap-2 overflow-y-auto pr-1">
              {icons.map((icon) => {
                const saving = savingPath === icon.source_path
                return (
                  <Tooltip key={icon.source_path}>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        disabled={savingPath != null}
                        onClick={() => void saveIcon(icon.source_path)}
                        aria-label={`Use ${icon.label}`}
                        className={cn(
                          'group grid min-w-0 gap-1.5 rounded-lg border border-border bg-panel p-2 text-left outline-none transition-[border-color,background-color,transform] hover:border-primary/60 hover:bg-panel2 focus-visible:border-primary focus-visible:ring-2 focus-visible:ring-ring/40 active:scale-[0.98]',
                          saving && 'border-primary bg-soft',
                        )}
                      >
                        <span className="relative grid aspect-square w-full place-items-center overflow-hidden rounded-md bg-background">
                          <img src={icon.data_url} alt="" className="size-[70%] object-contain" />
                          {saving && (
                            <span className="absolute inset-0 grid place-items-center bg-background/75">
                              <Loader2 size={15} className="animate-spin text-accent-text" />
                            </span>
                          )}
                        </span>
                        <span className="truncate text-center text-2xs text-sub group-hover:text-foreground">
                          {icon.label.split('/').at(-1)}
                        </span>
                      </button>
                    </TooltipTrigger>
                    <TooltipContent side="top" className="font-mono text-2xs">
                      {icon.label}
                    </TooltipContent>
                  </Tooltip>
                )
              })}
            </div>
          )}
        </div>

        <DialogFooter className="items-center justify-between border-t border-border bg-panel/35 px-5 py-3 sm:justify-between">
          <Button
            variant="ghost"
            size="sm"
            disabled={!current?.custom || savingPath != null}
            onClick={() => void useAutomaticIcon()}
          >
            {savingPath === 'automatic'
              ? <Loader2 size={13} className="animate-spin" />
              : <RotateCcw size={13} />}
            Use automatic icon
          </Button>
          <Button size="sm" disabled={savingPath != null} onClick={() => void chooseFile()}>
            <FolderOpen size={13} />
            Choose image
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
