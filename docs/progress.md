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

## 2026-11-09 — 2.4 CI on GitHub Actions
Done: `.github/workflows/ci.yml` (Ubuntu 26.04; job "check": fmt, clippy -D warnings, test, all `--locked`;
job "deps": cargo-deny) and `deny.toml`. Actions pinned to commit SHAs; token is read-only. Licence allow-list
generated from Cargo.lock (428 Linux crates, 8 licences). Decision: cargo-deny only (covers the RustSec database
that cargo-audit uses). First PR: both jobs green; merged with "Rebase and merge" as 3c6085f.
Reviewed advisories: 5 "unmaintained" (none vulnerable), all via Slint 1.18.1 — RUSTSEC-2026-0370/0371/0372
(zbus split crates, accessibility), 0192 (ttf-parser, Wayland title bar), 0441 (pin-utils, build-time). Ignored with
reasons in deny.toml; remove each when cargo-deny reports it unused.
Lessons: merge on GitHub before deleting branches / pulling; squash-merge would erase red/green commits.
Next: optional branch protection (required checks), then 2.6 licence/README/SECURITY.md.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.6 Licence, README, SECURITY.md
Done: `LICENSE` (official GPL-3.0 text, added by the human from an official copy), `README.md` (what it is, honest
status, build steps, how the project works, GPL-3.0-or-later notice, Slint credit), `SECURITY.md` (GitHub private
vulnerability reporting, 7/14-day response times, scope). Ruleset "protect main" created (required checks,
no force pushes); SOP added to CLAUDE.md: every change via branch + PR, Rebase and merge.
Next: 2.7 design tokens (light-first theme) — last task of M0.
Blockers: none.
Rules bent: none (docs only).

## 2026-11-11 — 2.7 Design tokens (light-first)
Done: approved Hybrid token board transcribed to `docs/design/tokens.md` (21 colours light + dark, rules, type,
shape, 16 contrast pairs) and `crates/app/ui/theme.slint` (`Theme` global, light). IBM Plex Sans 400/500/600 +
Mono 400 embedded from IBM's official releases with `OFL.txt` (both licence files compared identical; `fc-scan`
shows family "IBM Plex Sans"/"IBM Plex Mono"). Main window uses tokens, Plex, forced light (human checked: light
background, dark text). `tidewise_core::contrast` (WCAG ratio, strict hex parsing incl. multi-byte and "+" inputs)
and `crates/app/tests/theme_contrast.rs` (reads theme.slint, all 16 pairs >= 4.5:1, exactly 21 colours).
Decision record 0003.
Next: close M0, then M1 (Apps list, read-only).
Blockers: none.
Rules bent: cycle 1 (contrast) red was run and shown but not committed separately; the `#+F0000` test was added
in the green step after finding that `from_str_radix` accepts a leading "+".

## 2026-11-11 — 3.2 pacman fixtures
Done: real `LC_ALL=C pacman -Qi` output for 7 official-repo packages (base, zsh, linux-api-headers, tzdata, bc,
baobab, qt6-imageformats), one multi-record file, a short `-Qq` list filtered to those names, and a hand-made
truncated copy of zsh's record as the broken case, all in `tests/fixtures/pacman/` with a README (command,
pacman 7.1.0 / libalpm 16.0.1, date, why each file is there). git swapped for zsh at the human's request so no
AUR package names appear; every name in `Required By` / `Optional For` / `Optional Deps` checked against
`pacman -Slq`. Found: output to a pipe isn't line-wrapped (base's `Depends On` is one 261-char line).
Next: 3.3 parser for `pacman -Qi` / `-Qq` using these fixtures.
Blockers: none.
Rules bent: no red/green commits (data and docs only, no behaviour change).

## 2026-11-11 — 3.3 Parse `pacman -Qi` / `-Qq`
Done: `tidewise-pacman` now has `parse_qi` (name, version, description, installed size in bytes, install reason)
and `parse_qq`, as pure functions on `&str`. Records split at blank lines (last one needs no blank line), indented
continuation lines join the previous field, lines split at the first `:` only. Errors are all-or-nothing and name
the package and field (`ParseError`, hand-written, no new dependencies); `qi-broken-truncated.txt` gives
`missing field "Installed Size"` for `zsh`. Sizes use integer maths only (`B/KiB/MiB/GiB`, 1024-based, rounded to
the nearest byte, so they are what pacman printed, not exact). `is_valid_package_name` is an allow-list from
`man PKGBUILD` (`pkgname`): ASCII letters/digits and `@ . _ + -`, not starting with `-` or `.`; used by `-Qq` and
for `Name` in `-Qi`. clippy denies `unwrap`, `expect`, indexing and `panic!` in library code. 32 tests.
Next: 3.4 (run `pacman -Qi`/`-Qq` through `std::process::Command`, `LC_ALL=C`) and the Apps list view model.
Blockers: none.
Rules bent: red commits use a stub that compiles (so tests fail on an assertion). In cycles 3, 5, 7, 8 and 9
some new tests passed on first run (they pin behaviour or are a safety net); they are listed in the commit
messages, not hidden. One green commit (continuation lines) was committed with a compile error because the
test run was not gating the commit; I fixed and amended it locally before anything was pushed.
Not parsed yet (not needed for 3.3): `Depends On`, `Optional Deps`, `Required By`, other fields.

## 2026-11-11 — 3.7 Run `pacman -Qi` safely
Done: new `crates/pacman/src/run.rs` with one public entry point, `installed_packages() -> Result<Vec<InstalledPackage>,
PacmanError>`. It runs `/usr/bin/pacman -Qi` (no names = every package; 1490 records here, equal to the `-Qq`
count) with `Command` by absolute path, args as a list, `env_clear()` then `LC_ALL=C`, stdin null. Checked first
that an empty environment gives identical output (`env -i`). The work is split: `run` (start the process;
missing program -> `NotFound`, other start failure -> `Spawn`) and `packages_from_output(&Output)` (exit check,
`from_utf8_lossy`, `parse_qi`), unit-tested with fake `Output` values. `PacmanError` = `NotFound`, `Spawn`,
`Failed {code, stderr}` (stderr kept; `code: None` if killed by a signal), `Parse(ParseError)` (wrapped); hand-written
Display/source, no new dependencies. Invalid UTF-8 becomes U+FFFD but a bad `Name` is still rejected (both tested;
checked the tests fail if decoding is made strict). One `#[ignore]` test runs the real pacman
(`cargo test -p tidewise-pacman -- --ignored`) and cross-checks the count against `-Qq`.
Next: 3.4 Apps list view model using `installed_packages()`. Follow-up: **no timeout** on the pacman process
yet (a stuck pacman blocks the caller; also the call must not run on the UI thread); `/usr/bin/pacman` is
hard-coded, fine for Arch. `-Qq` has no runner yet (not needed).
Blockers: none.
Rules bent: cycles 4 and 5 tests (parse wrap, UTF-8) passed on first run because earlier code already did it;
committed as "pin" tests and mutation-checked. Red commits use a stub that compiles; some red failures are
"expected an error, got Ok" panics from a test helper. WBS sheet not updated (asked to skip). Intermediate green
commit 2 has a dead-code warning until `installed_packages` landed; the final commit is warning-free.

## 2026-11-11 — 3.8 Parse `Required By`
Done: `InstalledPackage` has `required_by: Vec<String>`. `parse_required_by` (qi.rs) splits the value on any
whitespace (pacman's double spaces and `\n` from joined continuation lines alike); `None` or a missing line gives
an empty list; every name goes through `is_valid_package_name`, and the first bad one is
`BadValue { package, field: "Required By", value: <that name> }` (all-or-nothing). Fixture coverage: zsh = one
name, qt6-imageformats = two names, base/bc/baobab = `None`, multi = per-record lists, broken-truncated still
fails on `Installed Size`. Inline test: 7 names over 3 lines. 38 tests in the crate.
Next: 3.4 Apps list view model, using `installed_packages()` (3.7) and `required_by`. SI-unit size
formatting (MB = 1 000 000 bytes) belongs in the 3.4 view model, not the parser.
Blockers: none.
Rules bent: red commit used a stub; `none_is_empty` and `missing_required_by_is_empty` passed on the stub (they
pin behaviour) and are named in the red commit message. A package literally named `None` would be read as "no
one" (pacman has the same ambiguity; none exists in the repos).
