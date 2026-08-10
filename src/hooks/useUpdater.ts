import { create } from "zustand";
import { toast } from "sonner";
import { commands } from "@/lib/bindings";

export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "none"
  | "error";

interface UpdaterStore {
  state: UpdateState;
  /** Version string of the pending update, once one is found. */
  version: string | null;
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

async function runInstall(
  version: string,
  set: (s: Partial<UpdaterStore>) => void,
) {
  set({ state: "downloading", version });
  await installUpdate();
  set({ state: "ready" });
  toast(`Update ${version} installed - restarting...`);
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
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

async function installUpdate(): Promise<void> {
  const res = await commands.installUpdate();
  if (res.status === "error") throw new Error(res.error);
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
      onStatus(`Installing update ${version}`);
      // Deliberately not wrapped in withTimeout: downloading an installer over
      // a slow link legitimately takes longer than the check budget, and
      // abandoning it mid-write is how you get a half-installed app.
      await installUpdate();
      set({ state: "ready" });
      onStatus("Restarting to finish the update");
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
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
        state === "available"
      ) {
        return;
      }
      void check(true);
    }, intervalMs);
    return () => clearInterval(timer);
  },
}));
