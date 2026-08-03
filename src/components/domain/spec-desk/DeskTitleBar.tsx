import { useEffect } from "react";
import specDeskLogo from "@/assets/icons/specdesk.png";
import { WindowControls } from "@/components/domain/WindowControls";
import { AiProviderChip } from "./AiProviderChip";
import { SpecRefreshButton } from "./SpecRefreshButton";

/**
 * The Desk's own titlebar. Window decorations are off app-wide, so this draws
 * the drag region and the window buttons, matching the main window's chrome.
 */
export function DeskTitleBar({
  repoName,
  repoId,
}: {
  repoName: string;
  /** For the chip's run state: turning the AI off must not hide a live run. */
  repoId: string | null;
}) {
  useEffect(() => {
    // Nothing to set up; kept as the seam for future titlebar state.
  }, []);

  return (
    <div
      data-tauri-drag-region
      className="flex h-10 flex-none select-none items-center gap-2.5 border-b border-border bg-panel pl-3 pr-0"
    >
      {/* The window icon. Decorations are off, so this titlebar is the only
          place the Desk's mark appears on the window itself. The mark carries
          its own color, so it is drawn as an image rather than tinted. */}
      <img
        src={specDeskLogo}
        alt=""
        aria-hidden
        width={16}
        height={16}
        className="flex-none object-contain"
        style={{ width: 16, height: 16 }}
      />
      <span className="text-xs font-semibold text-foreground">Spec Desk</span>
      <span className="overflow-hidden text-ellipsis whitespace-nowrap text-2xs text-muted-foreground">
        {repoName}
      </span>

      <div className="ml-auto flex h-full items-center gap-2 pl-3">
        {/* Only once the repo is open -- a refresh with no repo has nothing to
            re-read, and a dead button in the titlebar reads as broken. */}
        {repoId && <SpecRefreshButton repoId={repoId} />}
        <AiProviderChip repoId={repoId} />
        <div className="flex h-full items-stretch">
          <WindowControls />
        </div>
      </div>
    </div>
  );
}
