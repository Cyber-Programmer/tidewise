# M5 — Tests, onboarding, polish → v1.0 (≈15 hours)

## Context
Read `CLAUDE.md`. Feature freeze: no new features, only finishing and fixing.

## Tasks
1. First-run flow (design A): welcome, your PC, admin setup or skip, privacy + update toggle, add to
   app menu, first scan (design B).
2. Light theme + "Follow system" (design: Home light theme, tokens.md light column).
3. Settings screen v1 sections: Appearance, Behaviour, Cleaning (keep versions, log days, excluded
   folders, reminder), Admin access, Privacy & updates, About.
4. Update check: daily GitHub Releases check, download, verify minisign signature, swap with backup.
5. Every screen's empty/loading/error states from artboard F; keyboard navigation pass; contrast check.
6. Full distro container run + manual VM checklist (GNOME + KDE, X11 + Wayland), security review
   refresh, CHANGELOG, screenshots in README. Tag v1.0.

## Acceptance
- Nothing from artboards 1–6, A–F (v1 scope) left unbuilt or mismatched.
- Fresh Ubuntu 24.04 and Arch VMs: download → run → first run → clean → uninstall works with no terminal.
