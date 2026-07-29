# Tasks

## 1. Linking

- [ ] 1.1 Store branch→change links in git config (`branch.<name>.gitwyrm-spec`);
      link/unlink from the Desk header and the branch context menu
- [ ] 1.2 Infer a link when a branch's commits carry `Spec:` trailers and no explicit link exists

## 2. Commit form

- [ ] 2.1 Show the `Spec: <id>` trailer under the message box on linked branches,
      marked "added automatically", with a one-click remove for this commit
- [ ] 2.2 AI commit-message generation includes the trailer

## 3. Graph

- [ ] 3.1 Spec chip on commit rows whose message has a `Spec:` trailer
- [ ] 3.2 Branch-tip chip variant with live `n/m` progress
- [ ] 3.3 Chip click opens the Spec Desk at that change
- [ ] 3.4 ✦ AI marker on commits with an `Assisted-by:` trailer

## 4. Verify

- [ ] 4.1 Commit on a linked branch → chip appears without refresh
- [ ] 4.2 Tick a task → tip chip count updates
- [ ] 4.3 Unlinked branches show no trailer and no chips
