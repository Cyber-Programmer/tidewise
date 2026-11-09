# Progress log

Newest entry at the bottom. One entry per task: `## YYYY-MM-DD — <WBS id> <task>`, then Done / Next / Blockers /
Rules bent. Read the last entry before starting work.

## 2026-11-08 — 1.1–1.3 Plan v2
Done: Plan v2 approved (Arch-only rebuild, AUR packaging, repo-only installer, Rust + Slint, v0.1 after M4,
Update system in scope). Rule files rewritten. WBS sheet archived and rebuilt.
Next: reset the repo.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.1 Reset repo history
Done: old `main` (fbd8a33) saved as annotated tag `archive/old-v1`; new orphan branch with root commit
f87dc05 (README + `.gitignore`); `main` replaced with `--force-with-lease=main:fbd8a33`; dependabot branch
deleted; `git branch -r` shows only `origin/main`; no releases on GitHub.
Next: 2.2 workspace.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.2 Cargo workspace skeleton
Done: workspace with `tidewise-core`, `tidewise-pacman`, `tidewise-app` (binary `tidewise`), edition 2024,
resolver 3, shared `[workspace.package]` (b0600bc). First TDD cycle: `core::tests::app_name_is_tidewise` shown
failing (`left: ""`, fe542f7) then passing (9a4a1d4). Cargo's sample code removed from pacman (dcd14b3).
fmt / clippy -D warnings / test --all clean. Toolchain: rustc 1.99.1.
Lessons: `cargo new` made a nested `.git` in `crates/core` (rust-lang/cargo#11946) → use `--vcs none`.
Typos `[worksapce]` / `respository` caused "workspace has no members" → treat Cargo warnings as errors.
Next: 2.3 window.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.3 Empty Slint window
Done: Slint 1.18.1 added without default features (no system tray; `cargo tree -i ksni` finds nothing),
slint-build as build-dependency, tidewise-core as path dependency (6b541ad). `build.rs` + `ui/main.slint` +
`main.rs`: window titled "Tidewise" showing `tidewise_core::app_name()` (35390a9). Opened with FemtoVG and with
`SLINT_BACKEND=winit-software`; no terminal errors (human screenshots). fmt / clippy / test clean.
Notes: window renders dark because Slint follows the desktop theme; light-first theme comes in 2.7.
Next: 2.5 repo docs, then 2.4 CI.
Blockers: none.
Rules bent: no automated test for the window itself (a real window can't open in `cargo test`); checked
manually. The text it shows comes from a tested function.

## 2026-11-09 — 2.5 Repo docs
Done: `CLAUDE.md`, `docs/PLAN.md`, `docs/ENGINEERING.md`, this log, decision records 0001 and 0002 written into
the repo from the claude.ai project copies, plus "Known traps". D10: repo docs are now the source of truth.
Next: 2.4 CI on GitHub Actions, learned through a pull request.
Blockers: none.
Rules bent: none (docs only, no behaviour change).
