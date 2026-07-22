import { useSyncExternalStore } from "react";

export interface RepositoryPreview {
  dataUrl: string;
  width: number;
  height: number;
}

const MAX_CACHED_PREVIEWS = 24;
const previews = new Map<string, RepositoryPreview>();
const listeners = new Set<() => void>();

function emitChange() {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function setRepositoryPreview(
  repoId: string,
  preview: RepositoryPreview,
) {
  previews.delete(repoId);
  previews.set(repoId, preview);

  while (previews.size > MAX_CACHED_PREVIEWS) {
    const oldest = previews.keys().next().value;
    if (oldest == null) break;
    previews.delete(oldest);
  }

  emitChange();
}

export function useRepositoryPreview(
  repoId: string,
): RepositoryPreview | null {
  return useSyncExternalStore(
    subscribe,
    () => previews.get(repoId) ?? null,
    () => null,
  );
}
