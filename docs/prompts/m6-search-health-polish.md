# M6 — Search, Health page, onboarding, polish → v1.0 (≈25 hours)

## Context
Read `CLAUDE.md` (test-driven). Feature freeze after task 2: only finishing and fixing.

## Tasks
1. Search (Ctrl K, design 12): one index over apps, running processes, files by name in the home
   folder (incremental, low priority), actions and settings; keyboard-first. Unit tests for ranking.
2. Health page (design 11, "Why 82?"): each rule with its points, the check behind it, and a fix
   button to the right screen; drive health check via the helper only if asked.
3. First-run flow (design A): welcome, your PC, admin setup (installs the bundled helper) or skip,
   privacy + update toggle, add to app menu, first scan (design B).
4. Light theme + "Follow system"; Settings screen (design 10) complete.
5. Update check: daily GitHub Releases check, download, verify minisign signature, swap with backup.
6. Every screen's empty/loading/error states (artboard F); keyboard navigation pass; contrast check.
7. Full container run (Arch, Ubuntu 22.04, 24.04, Debian 12, Fedora) + manual VM checklist (GNOME +
   KDE, X11 + Wayland), security review refresh, CHANGELOG, README screenshots. Tag v1.0.0.

## Acceptance
- Nothing on the design canvas left unbuilt or mismatched.
- Fresh Ubuntu 22.04, Fedora and Arch VMs: download → run → first run → clean → uninstall works with
  no terminal.
