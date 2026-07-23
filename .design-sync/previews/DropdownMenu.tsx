import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuGroup,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  Button,
} from 'gitwyrm-mockup'
import { GitBranch, Check, ArrowUpDown, Trash2, Pencil, GitMerge } from 'lucide-react'

export function BranchActions() {
  return (
    <div style={{ padding: 20 }}>
      <DropdownMenu defaultOpen modal={false}>
        <DropdownMenuTrigger asChild>
          <Button variant="outline">
            <GitBranch /> feature/commit-graph
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <DropdownMenuLabel>feature/commit-graph</DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuGroup>
            <DropdownMenuItem>
              <Check /> Checkout
            </DropdownMenuItem>
            <DropdownMenuItem>
              <GitMerge /> Merge into main
            </DropdownMenuItem>
            <DropdownMenuItem>
              <Pencil /> Rename
              <DropdownMenuShortcut>F2</DropdownMenuShortcut>
            </DropdownMenuItem>
          </DropdownMenuGroup>
          <DropdownMenuSeparator />
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <ArrowUpDown /> Push to
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem>origin</DropdownMenuItem>
              <DropdownMenuItem>upstream</DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive">
            <Trash2 /> Delete branch
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

export function SortAndFilter() {
  return (
    <div style={{ padding: 20 }}>
      <DropdownMenu defaultOpen modal={false}>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost">
            <ArrowUpDown /> Sort commits
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          onEscapeKeyDown={(e) => e.preventDefault()}
          onPointerDownOutside={(e) => e.preventDefault()}
        >
          <DropdownMenuLabel>Sort by</DropdownMenuLabel>
          <DropdownMenuRadioGroup value="date">
            <DropdownMenuRadioItem value="date">Newest first</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="author">Author</DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="topo">Topological</DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
          <DropdownMenuSeparator />
          <DropdownMenuLabel>Show</DropdownMenuLabel>
          <DropdownMenuCheckboxItem checked>Remote branches</DropdownMenuCheckboxItem>
          <DropdownMenuCheckboxItem checked>Tags</DropdownMenuCheckboxItem>
          <DropdownMenuCheckboxItem>Stashes</DropdownMenuCheckboxItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
