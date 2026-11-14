# Tidewise — rules for Claude Code (v2, Arch-only rebuild)

Tidewise is an open-source "PC manager" for **Arch Linux**: install apps (official repos), update the system,
remove apps, clean junk, show disk usage, live performance and processes — in plain language, with technical
details one toggle away. Shipped as an **AUR package (PKGBUILD)**. Rust (edition 2024) + Slint 1.18.
Licence: GPL-3.0-or-later.

- Plan, decisions, milestones: `docs/PLAN.md` (source of truth — the claude.ai project copy follows it)
- Engineering practices: @docs/ENGINEERING.md
- Progress log: `docs/progress.md` — read the last entry before starting, add one when done
- Decisions: `docs/decisions/NNNN-title.md`
- WBS sheet: https://docs.google.com/spreadsheets/d/1gRI9jVQAm3w0y9UL2aFld0kT-kqiUSD9oMUV5yTX3Gc (tab **WBS**)

## The human is learning this codebase

The rebuild exists so the human understands every part. For each task:
- Before coding: a short plan in plain words (what, why, files, tests). Wait for approval.
- After coding: explain what changed and why in a few bullets, and name one thing worth reading in the diff.
- Prefer simple, readable code over clever code. No unexplained abstractions.

## Golden rules (never break these)

1. **The app never runs as root.** Only `tidewise-helper` runs as root, started with `pkexec` and one JSON request.
2. **The helper only does what is on its allow-list** (`crates/helper-protocol`). Adding, widening or changing an
   action needs the human's explicit OK in this session. Stop and ask.
3. **No shell, ever.** `std::process::Command` by absolute path, arguments as a list, never `sh -c`.
   In the helper also clear the environment and set `LC_ALL=C`.
4. **No partial upgrades.** Never `pacman -Sy <pkg>`, never `-Sy` followed by `-S`. Install against the current
   sync database, or do a full `pacman -Syu <pkgs>`.
5. **Show before change.** Every change has a dry-run producing the exact list on the confirm screen; the helper
   re-checks that list right before acting.
6. **One password prompt per confirmed transaction.** Batch the whole transaction into one helper call.
7. **Never delete the user's own files.** User files go to Trash (freedesktop Trash spec). Only caches, logs and
   package caches are removed directly, and only after a confirm screen.
8. **Protected things get no destructive button:** `base`, installed kernels, bootloader, desktop session, audio,
   network, `pacman`, `systemd`, the journal. Refuse in code, not only in the UI.
9. **Never delete or "fix" `/var/lib/pacman/db.lck`.** Wait and explain instead.
10. **Never pass `--overwrite` or `-d`/`-dd`** with user-chosen input. **`--noconfirm` only for a transaction the
    helper has verified:** providers named explicitly, a `--print` dry run equal to the confirmed list, no removals.
    (Human's decision, 13 Nov 2026; see `docs/decisions/0005-helper-invocation.md` and `docs/security/threat-model.md`.)
11. **No network** except what pacman itself does, plus an optional update check. No telemetry.
12. AUR is out of scope for v1.0. When added: build as the normal user, never root; show the PKGBUILD first.

## How to work in this repo

- **One task per session.** Plan first, wait for approval, finish with tests, stop.
- Tests first: commit the failing test alone (`test(scope): … (red)`), then the code (`feat|fix(scope): …`).
- Ask before any new dependency (why it's needed, size, maintenance) and before touching `crates/helper*`,
  `packaging/polkit`, `packaging/aur`, `deny.toml` or `.github/workflows/`. Add dependencies with `cargo add`.
- CI (`.github/workflows/ci.yml`) runs the same commands on every PR; merge only when it shows ✔.
- Small commits, Conventional Commits, never mix refactor with behaviour change.
- Commit messages: a Conventional Commits title (≤ 72 chars), a blank line, then a body for every
  non-trivial commit: what changed, why, tests added or changed, any rule bent. Wrap at 72.
  Task summary goes in the PR description, not in a commit: at the end of each task, write it to
  `/tmp/tidewise-pr-<WBS id>.md` (what each commit did, test counts, real-pacman findings, fuzz
  results, rules bent, how to verify) and give the human the exact
  `gh pr create --base main --title "…" --body-file /tmp/tidewise-pr-<WBS id>.md` command.
- **Claude Code commits on the task branch; the human pushes.** Never push, never force-push, never amend or
  rebase a pushed commit. `.claude/settings.json` enforces the common cases (deny `git push`, `sudo`, `rm -rf`…);
  those rules match command text and are not a security boundary, so follow the intent, not the letter.
- Start each task on a new branch from an up-to-date `main` (`git switch main`, then `git switch -c <prefix>/<task>`;
  the human runs `git pull` first).
- **Every change reaches `main` through a branch + pull request**, even docs. `main` is protected by the
  ruleset "protect main" (required checks: `fmt, clippy, test` and `cargo-deny`; force pushes blocked).
  Branch names: `feat/…`, `fix/…`, `docs/…`, `ci/…`, `chore/…`, `spike/…` (throwaway prototypes: pushed so a
  decision record can cite their SHAs, never merged, deleted once the decision is built). Merge with
  **Rebase and merge** (never squash: it would erase the red/green commits), delete the branch on GitHub, then
  `git pull` + `git fetch --prune`.
- Unsure how pacman behaves? Say so and propose a fixture or container test. Don't guess.
- End each task by appending to `docs/progress.md`: `## YYYY-MM-DD — <WBS id> <task>` then `Done:`, `Next:`,
  `Blockers:`, `Rules bent:`.

## Known traps (learned the hard way)

- `cargo new` into a folder whose parent doesn't exist yet creates a **nested `.git`** even inside this repo
  (rust-lang/cargo#11946). Always use `cargo new --vcs none …`, then check `find crates -name .git` is empty.
- A misspelled key in `Cargo.toml` (e.g. `respository`) is only a **warning**; a misspelled section
  (`[worksapce]`) silently drops everything in it. Treat every Cargo warning as an error.
- The first `cargo clippy` after adding a dependency rebuilds everything in "check" mode (minutes). That's normal.
- Slint follows the desktop's dark/light setting by default. The design is light-first: set it deliberately.
- **A pipe hides the exit code.** `cargo test | tail -5 && git commit …` commits even when tests fail, because
  `&&` sees `tail`'s success. Never gate a commit on a piped command: run the check on its own (or with
  `set -o pipefail`), read the result, then commit. (Happened once in 3.3; amended before push.)
- **Test the whole workspace, not one crate.** Run `cargo test --all` (the whole workspace), not `-p <crate>`,
  before every commit: a change in one crate can break another (4.3: `BadQuery` broke the app's exhaustive
  match for 4 commits).

## Workspace layout

```
crates/core             tidewise-core: scanning, sizes, live stats — no UI, no root
crates/pacman           tidewise-pacman: read pacman data (installed, search, info, dry-runs) — no root
crates/names            tidewise-names: package/repo name allow-lists, shared by pacman and helper-protocol — no deps
crates/helper-protocol  tidewise-helper-protocol: request/response types, validation, summary — serde, no root, no I/O
crates/helper           (M3) tidewise-helper: validates and runs allow-listed jobs as root
crates/app              tidewise-app: Slint UI + view models; binary `tidewise`; UI in crates/app/ui/*.slint
packaging/aur           (M3) PKGBUILD (installs app, helper, polkit policy)
packaging/polkit        (M3) io.github.tidewise.helper.policy
tests/fixtures          saved real pacman and /proc output
tests/container         Arch container tests (anything that runs pacman for real)
```

Keep `core` and `pacman` free of UI types. The UI talks to them through view models.
Slint is built without default features (no system tray): see `docs/decisions/0002-slint-features.md`.

## Reading pacman data (CLI, decision 0004)

Decided: run pacman and parse its text, not libalpm (`docs/decisions/0004-pacman-data-source.md`).
Force `LC_ALL=C` and parse machine-friendly output: `pacman -Qq`, `pacman -Qi <name>`, `pacman -Qdtq` (orphans),
`pacman -Ss`, `pacman -Si`, dry-runs with `--print`. Every parser gets fixture tests from real saved output,
including a broken case.

- `pacman -Ss` exits 1 with empty output when nothing matches: that is "no results", not an error.
- `pacman -Si a b` with one unknown name still prints the known ones and exits 1: keep the packages, report the name.
- Sizes come as rounded text (`358.23 KiB`); keep pacman's own words in the technical details.

## Commands (all must pass before "done"; run them, don't assume)

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo deny check                       # advisories (RustSec), licences, bans, sources — config in deny.toml
cargo build --release -p tidewise-app
```

### Container tests (rootless Podman; not part of "done" or CI yet)

Anything that runs pacman for real, as root, runs only in the test container (`tests/container/README.md`; one-time
setup there). Never on the host, never `--privileged`, never with a host mount except the built test binary, read-only.
Every container test starts with `require_container()`; the tests are `#[ignore]`, so `cargo test --all` skips them.

```
tests/container/run.sh                  # builds the image (cached) and the test binary, runs them with --network=none
```

### Fuzzing (nightly only, local; not part of "done" or CI yet)

`fuzz/` is its own workspace (cargo-fuzz), so the commands above never see it. Nightly is used only as
`cargo +nightly …`: never change the default toolchain. Seeds are `tests/fixtures/pacman/` (read-only).
Details in `fuzz/README.md`.

```
cargo fmt --manifest-path fuzz/Cargo.toml --check       # root fmt doesn't cover fuzz/
cargo +nightly fuzz build
mkdir -p fuzz/corpus/qi fuzz/corpus/qq fuzz/corpus/ss fuzz/corpus/si fuzz/corpus/qu fuzz/corpus/request
cargo +nightly fuzz run qi fuzz/corpus/qi tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
cargo +nightly fuzz run qq fuzz/corpus/qq tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
cargo +nightly fuzz run ss fuzz/corpus/ss tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
cargo +nightly fuzz run si fuzz/corpus/si tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
cargo +nightly fuzz run qu fuzz/corpus/qu tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
cargo +nightly fuzz run request fuzz/corpus/request tests/fixtures/helper-protocol -- -max_total_time=600; echo "exit: $?"
```

Clean run: ends with `Done N runs in 600 second(s)`, `exit: 0`, nothing in `fuzz/artifacts/`. A crash becomes a
normal stable unit test in `crates/pacman` (red first), then the fix.

## Definition of done

1. Test shown failing, then passing (UI-only work: a manual check recorded in progress.md instead, as a bent rule).
2. All commands above exit 0 on the final commit.
3. `docs/progress.md` entry written; any bent rule listed with the reason.
4. WBS sheet updated (below). 5. Final message ends with how to run it: `cargo run -p tidewise-app`.

## Tracking (WBS sheet, tab WBS, columns A–O)

- Find rows by WBS ID in column A (read A:A first; never assume a row number).
- Status (G): `To Do`, `In Progress`, `Done`, `Needs Verification`. `Done` only with evidence in L (commit SHAs +
  test names). Needs a person or a real machine to check → `Needs Verification`.
- Dates in H/I only when they happened. Never invent an owner (J); never mark an owner-"Human" row Done
  without the human's confirmation.
- New work: insert below its milestone's last row, next free ID, Level `Task`, Workstream text copied exactly.
- Never edit column O, the Overview tab (except the B2 snapshot line), or the Archive tabs.
- Sheet unreachable → write intended changes to `docs/wbs-pending.csv` and say so. Never claim an update you didn't make.
