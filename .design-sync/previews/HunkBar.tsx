import { HunkBar } from 'gitwyrm-mockup'

export function Unstaged() {
  return (
    <div style={{ padding: 20 }}>
      <HunkBar
        text="@@ -14,7 +14,9 @@ export function loadDB() {"
        canPatch
        kind="unstaged"
        onApply={() => {}}
      />
    </div>
  )
}

export function Staged() {
  return (
    <div style={{ padding: 20 }}>
      <HunkBar
        text="@@ -42,3 +42,6 @@ function refreshSettings(module) {"
        canPatch
        kind="staged"
        onApply={() => {}}
      />
    </div>
  )
}

export function WithDiscard() {
  return (
    <div style={{ padding: 20 }}>
      <HunkBar
        text="@@ -1,5 +1,8 @@ import { cn } from '@/lib/utils'"
        canPatch
        kind="unstaged"
        onApply={() => {}}
        onDiscard={() => {}}
      />
    </div>
  )
}

export function Disabled() {
  return (
    <div style={{ padding: 20 }}>
      <HunkBar
        text="@@ -8,2 +8,4 @@ const px = size === 'sm' ? 19 : 26"
        canPatch
        kind="unstaged"
        disabled
        onApply={() => {}}
        onDiscard={() => {}}
      />
    </div>
  )
}
