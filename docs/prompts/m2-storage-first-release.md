# M2 — Storage + first public release v0.1 (≈20 hours)

## Context
Read `CLAUDE.md`. Still read-only except moving the user's own files to Trash, which runs as the user.

## Goal
Storage screen complete, Home suggestions working, and a signed AppImage published as v0.1.

## Tasks
1. Disk scanner in `core`: per-drive totals (statvfs), home-folder breakdown, biggest files, "not opened
   in 6 months" (atime may be unreliable with `noatime`; fall back to mtime and say so in details).
   Count inodes once, don't follow symlinks, stay on one filesystem. Runs in a background thread with
   progress and cancel. Tests on temp trees.
2. Storage screen: drive bar + legend, folder map (treemap), drill-in view with checkboxes, biggest
   files list (match design 4 including the Downloads drill-in).
3. Move to Trash (freedesktop Trash spec, as the user) with an Undo toast. Tests with a temp Trash dir.
4. Home suggestions + health score (junk total is a stub until M3; drive-full and big-memory
   suggestions are real). Score rules exactly as on the "Why 82?" artboard.
5. AppImage build script using `appimagetool` + the static `type2-runtime`. Build on the oldest base we
   support; launch-test on Ubuntu 24.04 with **no libfuse2** installed and on Arch.
6. Release pipeline: GitHub Actions builds the AppImage on tag, attaches `.sha256` and a minisign
   signature. Ask the human to create and hold the signing key; never commit it.
7. First-run "Add to app menu" step: writes `~/.local/share/applications/tidewise.desktop` + icon.

## Acceptance
- v0.1 tag produces a downloadable, signed AppImage that starts on Arch and Ubuntu 24.04.
- Storage numbers match `du -sx` within 1% on a test folder.
- CHANGELOG.md has a plain-language v0.1 entry.
