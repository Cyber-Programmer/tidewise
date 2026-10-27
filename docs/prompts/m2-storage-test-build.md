# M2 — Storage + first private test build (≈20 hours)

## Context
Read `CLAUDE.md` (test-driven: failing test first, every task). Still read-only except moving the
user's own files to Trash, which runs as the user. There is no public release until v1.0; this
milestone ends with a private test build for 3–5 friends.

## Goal
Storage screen complete, Home suggestions working, and a signed AppImage build that friends can run.

## Tasks
1. Disk scanner in `core`: per-drive totals (statvfs), home-folder breakdown, biggest files, "not opened
   in 6 months" (atime may be unreliable with `noatime`; fall back to mtime and say so in details).
   Count inodes once, don't follow symlinks, stay on one filesystem. Background thread with progress
   and cancel. Tests on temp trees first.
2. Storage screen: drive bar + legend, folder map (treemap), drill-in view with checkboxes, biggest
   files list (match design 4 including the Downloads drill-in).
3. Move to Trash (freedesktop Trash spec, as the user) with an Undo toast. Tests with a temp Trash dir.
4. Home suggestions + health score (junk total is a stub until M3; drive-full and big-memory
   suggestions are real). Score rules exactly as on the "Why 82?" artboard; unit-test each rule.
5. AppImage build script using `appimagetool` + the static `type2-runtime`, building on an
   **Ubuntu 22.04** base. The helper binary is bundled now (unused until M3). Launch-test on Ubuntu
   22.04, Ubuntu 24.04 with **no libfuse2** installed, and Arch.
6. Release pipeline: GitHub Actions builds the AppImage on tag, attaches `.sha256` and a minisign
   signature. Ask the human to create and hold the signing key; never commit it. Tags are
   pre-releases (`v0.1.0-test.1`) until v1.0.
7. First-run "Add to app menu" step: writes `~/.local/share/applications/tidewise.desktop` + icon.

## Acceptance
- A pre-release tag produces a signed AppImage that starts on Arch, Ubuntu 22.04 and 24.04.
- Storage numbers match `du -sx` within 1% on a test folder (integration test).
- `docs/test-build-notes.md` tells testers what to try and how to report problems.
