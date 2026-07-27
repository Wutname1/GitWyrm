import { useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  GitCommitHorizontal,
  Pencil,
  Sparkles,
  Upload,
} from "lucide-react";
import { toast } from "sonner";
import logoUrl from "@/assets/logo.png";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { ProfileSwitcher } from "@/components/domain/ProfileSwitcher";
import {
  DisabledHint,
  Tooltip,
  TooltipButton,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { plural } from "@/lib/gitDisplay";
import { branchSync } from "@/lib/branchActions";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useAiConfigured, useAiMutations } from "@/hooks/useAi";
import {
  useBranches,
  useCommitDetail,
  useCommitLog,
  useStatus,
} from "@/hooks/useGitQueries";
import { useGitMutations } from "@/hooks/useGitMutations";
import {
  useActiveRepo,
  useWorkspaceStore,
  type CommitButtonMode,
} from "@/stores/workspaceStore";
import { useUiStore } from "@/stores/uiStore";

export function CommitMessageForm() {
  const repo = useActiveRepo();
  const status = useStatus(repo?.id ?? null);
  const branches = useBranches(repo?.id ?? null);
  const log = useCommitLog(repo?.id ?? null);
  const m = useGitMutations(repo?.id ?? null);
  const [msg, setMsg] = useState("");
  const [desc, setDesc] = useState("");
  const [amend, setAmend] = useState(false);
  const [justGenerated, setJustGenerated] = useState(false);
  const generatedTimer = useRef<number | null>(null);

  const headSha = log.data?.pages[0]?.commits[0]?.sha ?? null;
  const headDetail = useCommitDetail(repo?.id ?? null, amend ? headSha : null);

  const ai = useAiMutations();
  const configured = useAiConfigured();
  const aiProvider = useWorkspaceStore((s) => s.aiProvider);
  const aiModel = useWorkspaceStore((s) => s.aiModel);
  const showSettings = useUiStore((s) => s.showSettings);
  const openModal = useUiStore((s) => s.openModal);
  const commitButtonMode = useWorkspaceStore((s) => s.commitButtonMode);
  const setCommitButtonMode = useWorkspaceStore((s) => s.setCommitButtonMode);
  const aiReady =
    aiProvider != null &&
    aiModel != null &&
    (configured.data ?? []).some((c) => c.id === aiProvider);

  const stagedCount = status.data?.staged.length ?? 0;
  const currentBranch =
    branches.data?.local.find((b) => b.is_head)?.name ?? "HEAD";
  const pushPending = m.push.isPending;
  const canAmend = headSha != null;
  // Amend can change just the message, so it does not require staged files.
  const hasWork = amend ? canAmend : stagedCount > 0;
  const canCommit =
    hasWork &&
    msg.trim().length > 0 &&
    !m.createCommit.isPending &&
    !pushPending &&
    !ai.generate.isPending;

  useEffect(
    () => () => {
      if (generatedTimer.current != null)
        window.clearTimeout(generatedTimer.current);
    },
    [],
  );

  // When amend is turned on with empty fields, pre-fill the previous message.
  useEffect(() => {
    if (!amend || !headDetail.data) return;
    setMsg((m) => (m.trim().length > 0 ? m : headDetail.data.summary));
    setDesc((d) => (d.trim().length > 0 ? d : headDetail.data.body));
  }, [amend, headDetail.data]);

  // Why the commit button is off, shown on hover. The button is disabled in
  // exactly these cases, so this is the only way the user learns what to fix.
  const commitBlockedReason = m.createCommit.isPending
    ? "Committing…"
    : pushPending
      ? "Pushing…"
      : ai.generate.isPending
        ? "Waiting for the generated message"
        : amend && !canAmend
          ? "No commit to amend"
          : !hasWork
            ? "Stage files to commit"
            : msg.trim().length === 0
              ? "Enter a commit message"
              : undefined;

  const doCommit = (mode: CommitButtonMode = commitButtonMode) => {
    if (!canCommit) return;
    m.createCommit.mutate(
      { summary: msg, description: desc, amend },
      {
        onSuccess: async () => {
          setMsg("");
          setDesc("");
          setAmend(false);
          if (mode !== "commit_push") return;
          // Amending a commit the remote already has leaves the branch
          // diverged (behind > 0), so a plain push would be rejected. Refetch
          // the sync state and route through the force-push choice, matching
          // the toolbar Push button, instead of firing a push we know fails.
          const fresh = await branches.refetch();
          const head = fresh.data?.local.find((b) => b.is_head);
          const behind = head ? branchSync(head).behind : 0;
          if (behind > 0) openModal("push-choice");
          else m.push.mutate();
        },
      },
    );
  };

  const doGenerate = () => {
    if (!aiReady) {
      showSettings("ai");
      return;
    }
    if (ai.generate.isPending || !repo || stagedCount === 0) return;
    setJustGenerated(false);
    ai.generate.mutate(
      { repoId: repo.id, provider: aiProvider!, model: aiModel! },
      {
        onSuccess: (r) => {
          setMsg(r.summary);
          setDesc(r.description);
          setJustGenerated(true);
          if (generatedTimer.current != null)
            window.clearTimeout(generatedTimer.current);
          generatedTimer.current = window.setTimeout(
            () => setJustGenerated(false),
            1400,
          );
        },
        onError: (e) => toast.error(String(e)),
      },
    );
  };

  const generating = ai.generate.isPending;
  const nothingToGenerateFrom = aiReady && (!repo || stagedCount === 0);

  return (
    <div className="flex-none border-t border-border bg-panel2 px-3 pb-[13px] pt-[11px]">
      <div className={cn("relative mb-[9px]", generating && "min-h-[116px]")}>
        <div className="relative mb-[7px] rounded-md">
          <Input
            value={generating ? "" : msg}
            onChange={(e) => setMsg(e.target.value)}
            disabled={generating}
            placeholder={generating ? "" : "Summary (required)"}
            className={cn(
              "h-auto bg-background py-2 pl-2.5 pr-9 text-xs transition-[opacity,filter] duration-200",
              generating && "opacity-[0.18] saturate-[0.35]",
            )}
          />
          {/* The wrapper takes over the button's absolute placement: a
              `display:contents` span would have no box to hover, leaving the
              hint unreachable. */}
          <DisabledHint
            disabled={nothingToGenerateFrom}
            reason="Stage files to generate a message"
            className="absolute right-1.5 top-1/2 z-20 -translate-y-1/2"
          >
            <TooltipButton
              onClick={doGenerate}
              // Clicking while unconfigured opens AI settings, which is useful, so
              // that stays live. With nothing staged there is nothing to read, so
              // the button switches off and says so on hover.
              disabled={generating || nothingToGenerateFrom}
              aria-label={
                generating
                  ? "Generating commit message"
                  : "Generate commit message with AI"
              }
              tooltip={
                generating
                  ? "Generating commit message"
                  : !aiReady
                    ? "Set up an AI provider to generate messages"
                    : "Generate commit message with AI"
              }
              className={cn(
                "flex size-6 items-center justify-center overflow-hidden rounded-[5px] border text-sub",
                // When disabled the wrapper supplies this placement instead.
                !nothingToGenerateFrom &&
                  "absolute right-1.5 top-1/2 z-20 -translate-y-1/2",
                generating
                  ? "wyrm-ai-trigger-active cursor-wait border-transparent"
                  : justGenerated
                    ? "border-primary/25 bg-soft text-accent-text"
                    : aiReady && stagedCount > 0
                      ? "cursor-pointer border-primary/50 bg-soft text-accent-text hover:border-primary hover:bg-primary hover:text-primary-foreground"
                      : "cursor-pointer border-transparent hover:bg-panel3 hover:text-foreground",
              )}
            >
              {justGenerated && !generating ? (
                <Check size={13} strokeWidth={2.3} />
              ) : (
                <Sparkles
                  size={13}
                  className={cn(generating && "wyrm-ai-spark")}
                />
              )}
            </TooltipButton>
          </DisabledHint>
        </div>
        <Textarea
          value={generating ? "" : desc}
          onChange={(e) => setDesc(e.target.value)}
          disabled={generating}
          placeholder={generating ? "" : "Extended description…"}
          rows={2}
          className={cn(
            "wyrm-commit-description w-full bg-background px-2.5 py-2 text-xs transition-[opacity,filter] duration-200",
            generating && "opacity-[0.18] saturate-[0.35]",
          )}
        />
        {generating && (
          <div className="wyrm-ai-stage" role="status" aria-live="polite">
            <div className="wyrm-ai-logo-wrap" aria-hidden="true">
              <div className="wyrm-ai-energy-ring" />
              <img src={logoUrl} alt="" className="wyrm-ai-logo" />
              <i className="wyrm-ai-eye wyrm-ai-eye-left" />
              <i className="wyrm-ai-eye wyrm-ai-eye-right" />
              <svg className="wyrm-ai-graph" viewBox="0 0 40 27">
                <path d="M5 5h18v17h12" />
                <circle cx="5" cy="5" r="3" />
                <circle cx="23" cy="5" r="3" />
                <circle cx="35" cy="22" r="3" />
              </svg>
            </div>
            <div className="wyrm-ai-stage-copy">
              <div className="wyrm-ai-stage-label">
                Generating commit message
              </div>
              <div className="wyrm-ai-stage-status">
                Reading {stagedCount} staged file{stagedCount === 1 ? "" : "s"}…
              </div>
              <div className="wyrm-ai-stage-detail">
                Writing a summary and description
              </div>
            </div>
          </div>
        )}
      </div>
      {(stagedCount > 0 || amend) && canAmend && (
        <label
          className={cn(
            "mb-[9px] flex cursor-pointer items-center gap-2 rounded-md border px-2.5 py-1.5 text-2xs transition-colors",
            amend
              ? "border-border bg-panel3 text-accent-text"
              : "border-border bg-panel3 text-sub hover:text-foreground",
          )}
        >
          <input
            type="checkbox"
            checked={amend}
            onChange={(e) => setAmend(e.target.checked)}
            className="size-3.5 accent-primary"
          />
          <Pencil size={12} strokeWidth={2} />
          <span className="font-semibold">Amend previous commit</span>
        </label>
      )}
      <ProfileSwitcher />
      <div
        className={cn(
          "flex h-[34px] w-full overflow-hidden rounded-md border transition-colors",
          canCommit
            ? "border-primary/50 bg-soft text-accent-text hover:border-primary"
            : "cursor-not-allowed border-transparent bg-panel3 text-muted-foreground",
        )}
      >
        <DisabledHint
          disabled={!canCommit}
          reason={commitBlockedReason}
          className="min-w-0 flex-1"
        >
          <button
            onClick={() => doCommit()}
            disabled={!canCommit}
            className={cn(
              "flex flex-1 items-center justify-center gap-2 text-[0.78125rem] font-semibold transition-colors",
              canCommit
                ? "cursor-pointer hover:bg-primary hover:text-primary-foreground"
                : "cursor-not-allowed",
            )}
          >
            {amend ? (
              <Pencil size={14} strokeWidth={2} />
            ) : commitButtonMode === "commit_push" ? (
              <Upload size={14} strokeWidth={2} />
            ) : (
              <GitCommitHorizontal size={15} strokeWidth={2} />
            )}
            {pushPending
              ? "Pushing…"
              : amend
                ? `Amend commit on ${currentBranch}`
                : commitButtonMode === "commit_push"
                  ? `Commit & push to ${currentBranch}`
                  : `Commit ${plural(stagedCount, "file")} to ${currentBranch}`}
          </button>
        </DisabledHint>
        <Tooltip>
          <DropdownMenu>
            <TooltipTrigger asChild>
              <DropdownMenuTrigger asChild>
                <button
                  aria-label="Change default commit action"
                  className={cn(
                    "flex w-[30px] items-center justify-center border-l transition-colors",
                    canCommit
                      ? "cursor-pointer border-primary/40 hover:bg-primary hover:text-primary-foreground"
                      : "cursor-pointer border-border/60 text-sub hover:bg-panel2 hover:text-foreground",
                  )}
                >
                  <ChevronDown size={14} strokeWidth={2.2} />
                </button>
              </DropdownMenuTrigger>
            </TooltipTrigger>
            <TooltipContent>Change default commit action</TooltipContent>
            <DropdownMenuContent align="end" className="w-56">
              <DropdownMenuLabel className="text-xs text-sub">
                Default commit button action
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={commitButtonMode}
                onValueChange={(v) =>
                  setCommitButtonMode(v as CommitButtonMode)
                }
              >
                <DropdownMenuRadioItem value="commit" className="text-xs">
                  <GitCommitHorizontal size={13} className="text-current" />
                  Commit only
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="commit_push" className="text-xs">
                  <Upload size={13} className="text-current" />
                  Commit & push
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </Tooltip>
      </div>
    </div>
  );
}
