import type { RepoActivity, RepoPickerSort } from "@/stores/workspaceStore";
import { pathKey } from "@/lib/paths";

/** The fields the repository picker's table sorts on. */
export interface SortableRepo {
  name: string;
  path: string;
  headBranch: string | null;
}

/**
 * Order repository rows by the chosen column.
 *
 * Activity sorts on issues plus pull requests, so "what needs attention" lands
 * at the top as one number rather than making the user choose which half
 * matters. Every comparison falls back to name, so rows with equal counts hold
 * a stable order instead of shuffling between renders.
 *
 * A repository with no counts sorts as zero, which puts never-scanned rows with
 * the quiet ones. That is the honest answer: GitWyrm does not know of any open
 * work there, and the header says when the counts were last checked.
 */
export function sortRepos<T extends SortableRepo>(
  repos: T[],
  sort: RepoPickerSort,
  activity: Record<string, RepoActivity>,
): T[] {
  const weight = (repo: SortableRepo) => {
    const counts = activity[pathKey(repo.path)];
    return (counts?.issues ?? 0) + (counts?.prs ?? 0);
  };
  const byName = (a: SortableRepo, b: SortableRepo) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

  return [...repos].sort((a, b) => {
    let order = 0;
    if (sort.key === "activity") order = weight(a) - weight(b);
    else if (sort.key === "branch")
      order = (a.headBranch ?? "").localeCompare(b.headBranch ?? "");
    else order = byName(a, b);
    if (order !== 0) return sort.desc ? -order : order;
    // The tiebreak stays A to Z in both directions. Flipping it with the
    // column would send equal rows backwards, which reads as the list
    // reshuffling for no reason the user can see.
    return sort.key === "name" ? 0 : byName(a, b);
  });
}
