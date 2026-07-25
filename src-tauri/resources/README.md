# Bundled tools

GitWyrm ships its own copies of git and gpg so someone on a clean Windows
machine can clone, commit, sign, and push without installing anything first.

The binaries are **not committed**. `.github/scripts/fetch-bundled-tools.sh`
downloads them fresh during each release build, so every release carries the
current upstream version rather than a snapshot that silently ages:

    git -> MinGit, the tree Git for Windows publishes for redistribution
    gpg -> the GnuPG subset carved out of the portable Git for Windows tree

Expected layout after a fetch:

    resources/git/cmd/git.exe
    resources/gpg/gpg.exe

In a dev build this directory holds only this file, and the app falls back to
the git and gpg on your PATH. At runtime a system install always wins over the
bundled copy - see `src-tauri/src/git/bundled.rs`.

This file exists so the bundler's `resources/**/*` glob always matches
something. Without it, a build with no fetched tools fails outright.
