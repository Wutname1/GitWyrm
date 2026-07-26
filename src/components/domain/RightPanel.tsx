import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/uiStore";
import { GithubContextPanel } from "./github/GithubContextPanel";
import { ChangesList } from "./commit-form/ChangesList";
import { CommitMessageForm } from "./commit-form/CommitMessageForm";

export function RightPanel() {
  const changesFocusNonce = useUiStore((s) => s.changesFocusNonce);

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
      <ChangesList />
      <CommitMessageForm />
    </div>
  );
}
