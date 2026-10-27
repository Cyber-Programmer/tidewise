# Progress log

Newest entry at the bottom. One entry per session, written by Claude Code at the end of a task.

## 2026-10-27 — Planning
Done: Design (21 artboards), build plan, CLAUDE.md, phase prompts.
Next: Phase 0, session 1 — workspace + `crates/core` LiveStats.
Blockers: none.

## 2026-10-27 — Overnight run 1 (plans)
Plan T1: cargo workspace (core, adapters, app), LICENSE, README, .gitignore, deny.toml, CI workflow; verify fmt/clippy/test.
Plan T2: `core::stats` — pure rate functions tested first (zero interval, wrap, first sample), sysinfo sampler, one real two-sample test.
Plan T3: `core::distro` — os-release parser from hand-written fixtures, tool detection by absolute path, desktop from env.
Plan T4: `adapters::pacman` — capture real fixtures with LC_ALL=C, parse -Qi/-Qq/-Qdtq, tests for normal/no-required-by/German/empty.
Plan T5: `adapters::flatpak` + `adapters::appimage` — parse list output, scan dirs via tempdir tests.
Plan T6–T8: Slint spike, Tauri spike, measure.sh with fake /proc fixture; numbers into ADR as DRAFT.
