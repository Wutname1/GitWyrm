import { useEffect } from "react";
import { toJpeg } from "html-to-image";
import { describeError, log } from "@/lib/log";
import { setRepositoryPreview } from "@/lib/repositoryPreviews";
import { useWorkspaceStore } from "@/stores/workspaceStore";

const CAPTURE_WIDTH = 420;
const CAPTURE_DELAY_MS = 700;

export function RepositoryPreviewCapture() {
  const activeRepoId = useWorkspaceStore((state) => state.activeRepoId);

  useEffect(() => {
    if (!activeRepoId) return;

    const root = document.querySelector<HTMLElement>(
      "[data-repository-preview-root]",
    );
    if (!root) return;

    let cancelled = false;
    let capturing = false;
    let captureAgain = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const scheduleCapture = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void capture(), CAPTURE_DELAY_MS);
    };

    const capture = async () => {
      timer = null;
      if (cancelled) return;
      if (capturing) {
        captureAgain = true;
        return;
      }

      const rect = root.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return;

      const scale = Math.min(1, CAPTURE_WIDTH / rect.width);
      const width = Math.max(1, Math.round(rect.width * scale));
      const height = Math.max(1, Math.round(rect.height * scale));
      const backgroundColor = getComputedStyle(root).backgroundColor;

      capturing = true;
      try {
        const dataUrl = await toJpeg(root, {
          backgroundColor,
          cacheBust: false,
          canvasHeight: height,
          canvasWidth: width,
          pixelRatio: 1,
          quality: 0.76,
          skipAutoScale: true,
        });

        if (
          !cancelled &&
          useWorkspaceStore.getState().activeRepoId === activeRepoId
        ) {
          setRepositoryPreview(activeRepoId, { dataUrl, width, height });
        }
      } catch (error) {
        log.warn(
          `repository preview capture failed: ${describeError(error)}`,
        );
      } finally {
        capturing = false;
        if (captureAgain && !cancelled) {
          captureAgain = false;
          scheduleCapture();
        }
      }
    };

    const mutationObserver = new MutationObserver(scheduleCapture);
    mutationObserver.observe(root, {
      attributes: true,
      characterData: true,
      childList: true,
      subtree: true,
    });

    window.addEventListener("resize", scheduleCapture);
    root.addEventListener("scroll", scheduleCapture, true);
    scheduleCapture();

    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      mutationObserver.disconnect();
      window.removeEventListener("resize", scheduleCapture);
      root.removeEventListener("scroll", scheduleCapture, true);
    };
  }, [activeRepoId]);

  return null;
}
