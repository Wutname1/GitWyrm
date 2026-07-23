import {
  Popover,
  PopoverTrigger,
  PopoverContent,
  Button,
  Input,
} from 'gitwyrm-mockup'
import { Cloud, GitBranch } from 'lucide-react'

export function RemoteDetails() {
  return (
    <div style={{ padding: 20 }}>
      <Popover defaultOpen modal={false}>
        <PopoverTrigger asChild>
          <Button variant="outline">
            <Cloud /> origin
          </Button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <div className="flex flex-col gap-3">
            <div>
              <div className="text-sm font-medium text-foreground">origin</div>
              <div className="text-xs text-muted-foreground">
                git@github.com:gitwyrm/gitwyrm.git
              </div>
            </div>
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>Fetched 4 minutes ago</span>
              <span>12 branches</span>
            </div>
            <div className="flex gap-2">
              <Button size="sm" variant="secondary">Fetch</Button>
              <Button size="sm" variant="ghost">Edit URL</Button>
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}

export function CreateBranch() {
  return (
    <div style={{ padding: 20 }}>
      <Popover defaultOpen modal={false}>
        <PopoverTrigger asChild>
          <Button variant="outline">
            <GitBranch /> New branch
          </Button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <div className="flex flex-col gap-3">
            <div className="text-sm font-medium text-foreground">Create branch</div>
            <Input placeholder="feature/diff-viewer" defaultValue="feature/diff-viewer" />
            <div className="text-xs text-muted-foreground">Based on main</div>
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost">Cancel</Button>
              <Button size="sm">Create</Button>
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
