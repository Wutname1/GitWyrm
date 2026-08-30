import { describe, expect, it } from "vitest";
import { pathKey } from "@/lib/paths";
import { sortRepos, type SortableRepo } from "@/lib/repoSort";
import type { RepoActivity } from "@/stores/workspaceStore";

const repo = (
  name: string,
  path: string,
  headBranch: string | null = null,
): SortableRepo => ({ name, path, headBranch });

const REPOS = [
  repo("zeta", "C:/code/zeta", "main"),
  repo("alpha", "C:/code/alpha", "develop"),
  repo("mid", "C:/code/mid", "release"),
];

// Keyed through pathKey, the same way the store records a scan: on Windows
// that normalizes the separators as well as the case.
const ACTIVITY: Record<string, RepoActivity> = {
  [pathKey("C:/code/zeta")]: { prs: 1, issues: 1 },
  [pathKey("C:/code/alpha")]: { prs: 0, issues: 5 },
};

const names = (repos: SortableRepo[]) => repos.map((r) => r.name);

describe("sortRepos", () => {
  it("orders by name, ignoring case", () => {
    const sorted = sortRepos(
      [...REPOS, repo("Beta", "C:/code/beta")],
      { key: "name", desc: false },
      {},
    );
    expect(names(sorted)).toEqual(["alpha", "Beta", "mid", "zeta"]);
  });

  it("reverses when descending", () => {
    const sorted = sortRepos(REPOS, { key: "name", desc: true }, {});
    expect(names(sorted)).toEqual(["zeta", "mid", "alpha"]);
  });

  it("adds issues and pull requests together for the activity column", () => {
    const sorted = sortRepos(REPOS, { key: "activity", desc: true }, ACTIVITY);
    expect(names(sorted)).toEqual(["alpha", "zeta", "mid"]);
  });

  it("treats a repository with no counts as having nothing open", () => {
    const sorted = sortRepos(REPOS, { key: "activity", desc: true }, {});
    // Everything ties at zero, so the name fallback decides.
    expect(names(sorted)).toEqual(["alpha", "mid", "zeta"]);
  });

  it("breaks activity ties by name rather than leaving the order to chance", () => {
    const tied: Record<string, RepoActivity> = {
      [pathKey("C:/code/zeta")]: { prs: 2, issues: 0 },
      [pathKey("C:/code/alpha")]: { prs: 0, issues: 2 },
      [pathKey("C:/code/mid")]: { prs: 1, issues: 1 },
    };
    const sorted = sortRepos(REPOS, { key: "activity", desc: true }, tied);
    expect(names(sorted)).toEqual(["alpha", "mid", "zeta"]);
  });

  it("orders by branch name", () => {
    const sorted = sortRepos(REPOS, { key: "branch", desc: false }, {});
    expect(names(sorted)).toEqual(["alpha", "zeta", "mid"]);
  });

  it("leaves the input array untouched", () => {
    const input = [...REPOS];
    sortRepos(input, { key: "name", desc: false }, {});
    expect(names(input)).toEqual(["zeta", "alpha", "mid"]);
  });
});
