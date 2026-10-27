# M4 — Apps across 6 sources + uninstall + End task (≈35 hours)

## Context
Read `CLAUDE.md` (test-driven). Helper exists from M3; this milestone adds `RemovePackages` and
`RemoveOldSnaps`, which need the human's explicit approval of the exact variants and validation
before coding. Ends with private test build 2.

## Goal
One app list across pacman, apt, dnf, Flatpak, Snap and AppImage; uninstall with all its cases
(design 3 + flow D); End task in Processes (flow E).

## Tasks
1. Adapter trait `AppSource { list(), size(), removal_plan(app), remove(plan) }`. Write fixture tests
   first, then implement: pacman, apt (dpkg-query), dnf (rpm queries), Flatpak (user + system), Snap
   (`snap list --all`), AppImage (files + `.desktop` + icon). Map packages to "apps" via `.desktop`
   files so libraries don't flood the list.
2. Removal plans as dry-runs: pacman `-Rns --print`, apt `-s remove`, dnf `remove --assumeno`, flatpak
   removal + `--unused` preview, snap revisions. Detect **blocked** (required by others, essential,
   protected) and **busy** (package-manager lock held).
3. Helper: `RemovePackages { manager, names }` with strict name pattern, `--` before names, re-check
   installed, never force; `RemoveOldSnaps` computes disabled revisions itself. Hostile-input tests first.
   Flatpak user installs and AppImages are removed without the helper.
4. Apps screen + uninstall panel + flow D (confirm, removing, done, blocked, busy); "keep settings"
   default on; settings paths per source shown before deleting and moved to Trash.
5. End task: SIGTERM to the app's process group, wait 5 s, offer Force quit (SIGKILL); protected
   processes refused in code. Toast with memory freed.
6. Add the Fedora container to CI; integration-test one uninstall per source.
7. Private test build 2 (`v0.1.0-test.2`).

## Acceptance
- Can't remove a package that something else needs; can't remove protected packages (tested).
- Uninstall integration tests green in Arch, Ubuntu 22.04, Debian 12 and Fedora containers.
