import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/uiStore";
import { useActiveRepo } from "@/stores/workspaceStore";
import { GithubContextPanel } from "./github/GithubContextPanel";
import { SpecCard } from "./SpecCard";
import { ChangesList } from "./commit-form/ChangesList";
import { CommitMessageForm } from "./commit-form/CommitMessageForm";

export function RightPanel() {
  const changesFocusNonce = useUiStore((s) => s.changesFocusNonce);
  const repo = useActiveRepo();

  const [flash, setFlash] = useState(false);
  const headerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (changesFocusNonce === 0) return;
    headerRef.current?.scrollIntoView({ block: "nearest" });
    setFlash(false);
    const raf = requestAnimationFrame(() => setFlash(true));
    const timer = setTimeout(() => setFlash(false), 900);
    return () => {
      cancelAnimationFrame(raf);
      clearTimeout(timer);
    };
  }, [changesFocusNonce]);

  return (
    <div
      data-dim-on-drag
      className={cn(
        "flex h-full w-full min-h-0 flex-col border-l bg-panel transition-colors duration-500",
        flash ? "border-primary" : "border-border",
      )}
    >
      <GithubContextPanel />
      <SpecCard />
      <ChangesList />
      {/* Keyed by repo: the message box holds per-repo state (draft text, the
          in-flight AI generation). Without the key one instance is reused
          across tabs, so switching repos mid-generation carries the spinner
          and drops the message into whichever repo is on screen. */}
      <CommitMessageForm key={repo?.id ?? "none"} />
    </div>
  );
}
