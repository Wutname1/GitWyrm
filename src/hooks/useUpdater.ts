import { create } from "zustand";
import { toast } from "sonner";
import { getVersion } from "@tauri-apps/api/app";
import {
  commands,
  type ChangelogEntry,
  type InstallOutcome,
} from "@/lib/bindings";
import { clearSplashBar, setSplashBar } from "@/lib/splash";
import { useWorkspaceStore } from "@/stores/workspaceStore";

export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  /** Bytes are arriving; `progress` tracks how far along. */
  | "downloading"
  /** Downloaded and verified, waiting for the user to restart. */
  | "downloaded"
  | "ready"
  | "none"
  /**
   * An update exists but this install cannot apply it: a Linux .deb or .rpm
   * needs root, and every escalation path was refused or unavailable. Terminal
   * on purpose - the auto-check skips it, because re-offering an update the
   * user cannot install would nag them every couple of hours forever.
   */
  | "manual"
  | "error";

/** How far a download has got, for the modal's bar. */
export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

interface UpdaterStore {
  state: UpdateState;
  /** Version string of the pending update, once one is found. */
  version: string | null;
  /**
   * The running version, so the modal can show what the update moves away
   * from. Resolved when the modal opens, which is the only place it is shown.
   */
  currentVersion: string | null;
  /** Live download progress, or null when nothing is downloading. */
  progress: DownloadProgress | null;
  /**
   * Where to get the update by hand, set only in the "manual" state. Nothing
   * else populates this, so its presence is what tells the UI to offer a link
   * rather than a retry button.
   */
  manualUrl: string | null;
  /** Release notes for every version newer than the running one. */
  changelog: ChangelogEntry[];
  /** True while the changelog request is in flight. */
  changelogLoading: boolean;
  /** Whether the update modal is open. */
  modalOpen: boolean;
  /** Show the update details modal, and fetch notes if not already loaded. */
  openModal: () => void;
  /** Hide the modal. The download, if any, carries on. */
  closeModal: () => void;
  /**
   * Fetch and hold the installer without restarting, so the user can pick
   * their moment. Moves to 'downloaded' on success.
   */
  download: () => Promise<void>;
  /** Install what `download` fetched and restart into it. */
  restartAndInstall: () => Promise<void>;
  /**
   * Look for a newer release without installing it. On success, moves to
   * 'available' (with `version` set) or 'none'. `silent` suppresses the
   * toasts, for the automatic check that runs on launch.
   */
  check: (silent?: boolean) => Promise<void>;
  /** Download, install, and relaunch into the update found by `check`. */
  install: () => Promise<void>;
  /** Check and, if an update exists, install it in one shot (manual trigger). */
  checkAndInstall: () => Promise<void>;
  /**
   * The launch path, run while the splash is still covering the window: check
   * and, when auto-update is on, install and relaunch before the app is ever
   * shown. Reports progress through `onStatus` so the splash line can narrate
   * it, and never toasts -- there is no app behind the splash to toast onto.
   *
   * Resolves once it is safe to carry on booting. When an update does install
   * this never resolves in practice: the process relaunches instead.
   */
  runLaunchUpdate: (
    auto: boolean,
    onStatus: (message: string) => void,
  ) => Promise<void>;
  /**
   * Begin re-checking for updates every `intervalMs`. Returns a cleanup that
   * stops the timer. Once an update is found the timer stops on its own -- the
   * status-bar button is showing, so there is nothing more to look for.
   */
  startAutoCheck: (intervalMs: number) => () => void;
}

/** How often to look for a newer release while the app stays open: 2 hours. */
export const AUTO_CHECK_INTERVAL_MS = 2 * 60 * 60 * 1000;

// Dev builds carry the placeholder version from tauri.conf.json, so every
// release looks newer and would offer to replace the build being worked on.
function skipInDev(
  silent: boolean,
  set: (s: Partial<UpdaterStore>) => void,
): boolean {
  if (!import.meta.env.DEV) return false;
  set({ state: "none" });
  if (!silent) toast("Update checks are off in development builds");
  return true;
}

/**
 * Download and install, narrating progress in a toast.
 *
 * The toast is updated in place by id rather than stacking a new one per
 * chunk. On Windows `installUpdate()` never returns -- the process exits inside
 * it -- so the last thing the user sees here is the download completing, and
 * the helper window takes over from there.
 */
async function runInstall(
  version: string,
  set: (s: Partial<UpdaterStore>) => void,
) {
  set({ state: "downloading", version });

  const toastId = `update-${version}`;
  toast.loading(`Downloading update ${version}`, { id: toastId });

  const unlisten = await watchDownload(
    (message) => toast.loading(message, { id: toastId }),
    () => {},
  );
  let outcome: InstallOutcome;
  try {
    outcome = await installUpdate();
  } finally {
    unlisten();
  }

  // Installing a Linux package needs root, and the user can refuse. Nothing is
  // broken when they do, so say what to do next instead of showing an error.
  if (outcome.kind === "manual_required") {
    set({ state: "manual", version: outcome.version, manualUrl: outcome.url });
    toast.info(`Update ${outcome.version} has to be installed by hand`, {
      id: toastId,
      description: "GitWyrm could not get permission to install it for you.",
    });
    return;
  }

  set({ state: "ready" });
  toast.loading("Restarting to finish the update", { id: toastId });
}

/**
 * Ask the backend whether a newer build exists on the user's channel.
 *
 * Goes through our own command rather than the updater plugin's `check()`,
 * which rebuilds the updater from tauri.conf.json and so always reads the
 * stable manifest. This previously sent an `X-Update-Channel` header to a
 * static GitHub asset, which ignores headers -- and `/releases/latest` skips
 * prereleases besides, so choosing Beta in Settings had no effect at all.
 * Resolving the endpoint in Rust is what makes the setting real.
 */
async function fetchUpdate(): Promise<string | null> {
  const res = await commands.checkForUpdate();
  if (res.status === "error") throw new Error(res.error);
  return res.data;
}

async function installUpdate(): Promise<InstallOutcome> {
  const res = await commands.installUpdate();
  if (res.status === "error") throw new Error(res.error);
  return res.data;
}

/** Event name the backend emits download progress on. Matches updates.rs. */
const UPDATE_PROGRESS_EVENT = "update://progress";

interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** "4.2 MB", for the download line. */
function formatMb(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/**
 * Narrate the download, and hand back an unsubscribe.
 *
 * Reports through the same `onStatus`/bar pair the splash already uses, so this
 * works whether the caller is the launch splash or a future in-app surface.
 */
async function watchDownload(
  onStatus: (message: string) => void,
  onFraction: (fraction: number | null) => void,
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, ({ payload }) => {
    const { downloaded, total } = payload;
    if (total && total > 0) {
      onStatus(`Downloading ${formatMb(downloaded)} of ${formatMb(total)}`);
      onFraction(downloaded / total);
    } else {
      // No Content-Length: report what has arrived, and let the bar shuttle.
      onStatus(`Downloading ${formatMb(downloaded)}`);
      onFraction(null);
    }
  });
}

/**
 * Report raw download progress, and hand back an unsubscribe.
 *
 * Distinct from `watchDownload`, which formats a status line for the splash.
 * The modal draws its own bar and needs the numbers, not a sentence.
 */
async function watchDownloadProgress(
  onProgress: (progress: DownloadProgress) => void,
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<UpdateProgress>(UPDATE_PROGRESS_EVENT, ({ payload }) => {
    onProgress({ downloaded: payload.downloaded, total: payload.total });
  });
}

/**
 * How long the launch check may take before booting carries on without it. The
 * splash is covering the window for the whole wait, so an unreachable update
 * server has to cost a few seconds, not the whole startup.
 */
const LAUNCH_CHECK_TIMEOUT_MS = 8000;

/**
 * Resolve to `null` if `promise` has not settled within `ms`.
 *
 * Used only on the launch check, where the point is to stop *waiting* rather
 * than to stop the work: the backend command carries on and its result is
 * simply ignored, which is fine because a check has no side effects.
 */
async function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
): Promise<T | null> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<null>((resolve) => {
        timer = setTimeout(() => resolve(null), ms);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

/** Shared updater state so a launch check and the status-bar button agree. */
export const useUpdater = create<UpdaterStore>((set, get) => ({
  state: "idle",
  version: null,
  currentVersion: null,
  progress: null,
  manualUrl: null,
  changelog: [],
  changelogLoading: false,
  modalOpen: false,

  openModal: () => {
    set({ modalOpen: true });

    // Resolved on every open, and before the changelog guard below: the notes
    // are fetched once per session, so leaving this inside that branch would
    // skip it on a reopen and leave the header with no "from" version.
    if (!get().currentVersion) {
      void getVersion()
        .then((v) => set({ currentVersion: v }))
        // Header falls back to showing the target alone.
        .catch(() => {});
    }

    // Notes are fetched once per session. They cannot change under a running
    // app -- a new release would need a fresh check, which resets this anyway.
    if (get().changelog.length > 0 || get().changelogLoading) return;

    set({ changelogLoading: true });
    void (async () => {
      try {
        const current = await getVersion();
        // The target decides whether prerelease notes are relevant: heading to
        // a stable build supersedes them, heading to another beta does not.
        // Falls back to the running version, which reads as a same-channel
        // move and so keeps whatever notes exist.
        const res = await commands.changelogSince(current, get().version ?? current);
        // A missing changelog must not block the update: the modal shows the
        // version and its buttons regardless, just without notes.
        set({ changelog: res.status === "ok" ? res.data : [] });
      } catch {
        set({ changelog: [] });
      } finally {
        set({ changelogLoading: false });
      }
    })();
  },

  closeModal: () => set({ modalOpen: false }),

  download: async () => {
    if (skipInDev(false, set)) return;
    if (get().state === "downloading") return;

    set({ state: "downloading", progress: null });

    const unlisten = await watchDownloadProgress((progress) => set({ progress }));
    try {
      const res = await commands.downloadUpdate();
      if (res.status === "error") throw new Error(res.error);
      if (!res.data) {
        set({ state: "none", version: null, progress: null });
        return;
      }

      set({ state: "downloaded", version: res.data, progress: null });

      // Skip the second click when the user has asked us to.
      if (useWorkspaceStore.getState().autoRestartAfterDownload) {
        await get().restartAndInstall();
      }
    } catch (e) {
      set({ state: "error", progress: null });
      toast.error(`Download failed: ${(e as Error).message}`);
    } finally {
      unlisten();
    }
  },

  restartAndInstall: async () => {
    set({ state: "ready" });
    try {
      // Does not return on Windows: the process exits inside the handoff.
      const res = await commands.installDownloadedUpdate();
      if (res.status === "error") throw new Error(res.error);
    } catch (e) {
      set({ state: "error" });
      toast.error(`Update failed: ${(e as Error).message}`);
    }
  },

  check: async (silent = false) => {
    if (skipInDev(silent, set)) return;
    // Don't stomp an install already under way.
    if (get().state === "downloading") return;
    set({ state: "checking" });
    try {
      const version = await fetchUpdate();
      if (!version) {
        set({ state: "none", version: null });
        if (!silent) toast("GitWyrm is up to date");
        return;
      }
      set({ state: "available", version });
      if (!silent) toast(`Update ${version} is available`);
    } catch (e) {
      set({ state: "error" });
      if (!silent) toast.error(`Update check failed: ${(e as Error).message}`);
    }
  },

  install: async () => {
    if (skipInDev(false, set)) return;
    if (get().state === "downloading") return;
    set({ state: "checking" });
    try {
      const version = await fetchUpdate();
      if (!version) {
        set({ state: "none", version: null });
        toast("GitWyrm is up to date");
        return;
      }
      await runInstall(version, set);
    } catch (e) {
      set({ state: "error" });
      toast.error(`Update failed: ${(e as Error).message}`);
    }
  },

  checkAndInstall: async () => {
    await get().install();
  },

  runLaunchUpdate: async (auto, onStatus) => {
    // Dev builds always look out of date against a real release, so a launch
    // install would replace the build being worked on with a shipped one.
    if (import.meta.env.DEV) {
      set({ state: "none" });
      return;
    }

    set({ state: "checking" });
    onStatus("Checking for updates");
    try {
      // The splash is covering the window for this whole wait, so an
      // unreachable update server must cost a few seconds rather than the whole
      // startup. The timeout lives here now: the check runs in Rust, and losing
      // the race only abandons the wait, it does not cancel the command.
      const version = await withTimeout(fetchUpdate(), LAUNCH_CHECK_TIMEOUT_MS);
      if (!version) {
        set({ state: "none", version: null });
        return;
      }

      // Auto-update off: note the update and let the status-bar button offer
      // it, rather than holding the splash on something the user declined.
      if (!auto) {
        set({ state: "available", version });
        return;
      }

      set({ state: "downloading", version });
      onStatus(`Downloading update ${version}`);

      const unlisten = await watchDownload(onStatus, setSplashBar);
      let outcome: InstallOutcome;
      try {
        // Deliberately not wrapped in withTimeout: downloading an installer over
        // a slow link legitimately takes longer than the check budget, and
        // abandoning it mid-write is how you get a half-installed app.
        //
        // On Windows this call does not return: the updater hands the installer
        // to ShellExecute and then calls std::process::exit(0). The lines below
        // are for platforms whose install path does return -- and as a safety
        // net if that ever changes.
        outcome = await installUpdate();
      } finally {
        unlisten();
        clearSplashBar();
      }

      // The user declined the root prompt, or there was no way to ask. Carry on
      // into the app rather than holding the splash: they can install it later,
      // and nothing about the running version is broken.
      if (outcome.kind === "manual_required") {
        set({
          state: "manual",
          version: outcome.version,
          manualUrl: outcome.url,
        });
        return;
      }

      set({ state: "ready" });
      onStatus("Restarting to finish the update");
    } catch {
      // A failed update must never keep someone out of their repositories:
      // fall through to a normal boot and let them retry from Settings.
      set({ state: "error" });
    }
  },

  startAutoCheck: (intervalMs) => {
    const timer = setInterval(() => {
      const { state, check } = get();
      // Nothing to re-check once an update is already found or installing, and
      // don't stack a check on top of one still running.
      if (
        state === "checking" ||
        state === "downloading" ||
        state === "ready" ||
        state === "available" ||
        // Terminal: the update cannot be installed from inside the app, so
        // re-finding it every couple of hours would only nag someone who
        // already knows and can do nothing about it from here.
        state === "manual"
      ) {
        return;
      }
      void check(true);
    }, intervalMs);
    return () => clearInterval(timer);
  },
}));
