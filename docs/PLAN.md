# Tidewise build plan (summary)

The full, living plan is the "Tidewise — Build Plan" doc:
https://claude.ai/artifact/7WXEHCqPBgtcJSPKvHmm5r

Key points for quick reference:

- v1.0 scope (frozen): every designed screen — Home, Clean up, Apps, Storage, Performance, Processes,
  Startup apps, Services, History, Search, Health page, Settings, first run. One public release.
- Package managers: pacman, apt, dnf; app sources: Flatpak, Snap, AppImage.
- Minimum: Ubuntu 22.04 and equivalents; builds run on a 22.04 base.
- Stack: Rust. UI toolkit (Slint or Tauri) chosen by the Phase 0 spike, see
  `docs/decisions/0001-ui-toolkit.md`.
- Development: test-driven (failing test first), unit + integration tests in distro containers.
- Security: app runs as the user. The helper ships inside the AppImage and is installed once at setup
  to `/usr/local/libexec/tidewise-helper` (root can't execute it from inside the FUSE mount); it runs
  only allow-listed jobs behind polkit.
- Milestones: Phase 0 spike (Nov 2026) → M1 core + live screens → M2 Storage + private test build
  (end Jan 2027) → M3 Clean up + helper → M4 Apps, 6 sources (test build 2, May 2027) → M5 Startup,
  Services, History → M6 Search, Health, polish → v1.0 (mid-Aug 2027). ~175 hours.
- Prompts for each phase: `docs/prompts/`.
