# Tidewise — rules for Claude Code

Tidewise is an open-source "PC manager" for Linux, aimed at people switching from Windows/macOS.
It cleans junk, shows what uses disk space, lists and removes apps from every source, and shows
live performance and processes — in plain language, with technical details one toggle away.
Shipped as a single AppImage (minimum: Ubuntu 22.04 era; builds run on a 22.04 base).
Package managers: pacman, apt, dnf; app sources: Flatpak, Snap, AppImage. Licence: GPL-3.0-or-later.

- Build plan (milestones, security model, decisions): see `docs/PLAN.md` (link to the live doc inside)
- Design source of truth: `docs/design/` (screens as HTML, `tokens.md`)
- Current phase prompt: `docs/prompts/`
- Progress log: `docs/progress.md` — read the last entry before starting, add one when done
- Engineering practices (TDD, security testing, review, budgets): @docs/ENGINEERING.md

## Golden rules (never break these)

1. **The app never runs as root.** No `sudo`, no `pkexec` calls from the app except to start
   `tidewise-helper` with one JSON request.
2. **The helper only does what is on its allow-list** (`crates/helper-protocol`). Adding, widening or
   changing an action needs the human's explicit OK in this session. Stop and ask.
3. **No shell, ever.** Run programs with `std::process::Command` by absolute path, arguments as a list,
   never through `sh -c`. In the helper also clear the environment and set `LC_ALL=C`.
4. **Never delete the user's own files.** User files go to Trash (freedesktop Trash spec / `gio trash`).
   Only caches, logs and package caches are removed directly, and only after a confirm screen.
5. **Show before change.** Every destructive action has a dry-run that produces the exact list shown on
   the confirm screen, and the action re-checks that list before acting.
6. **No network** except the daily update check to GitHub Releases (toggle in Settings). No telemetry.
7. **Protected things get no destructive button:** essential/required packages, the desktop session,
   audio, network, the journal. Refuse in code, not only in the UI.
8. Never delete or "fix" a package-manager lock file (`/var/lib/pacman/db.lck`, dpkg locks). Wait instead.

## How to work in this repo

- Start every task by reading `docs/progress.md` (last entry) and the phase prompt.
- **Plan first.** Propose a short plan (files to touch, tests to add) and wait for approval before writing
  code. Exception: tiny fixes the human explicitly asks for.
- **One task per session.** Finish it with tests, then stop. Don't start the next task unasked.
- End each task by appending to `docs/progress.md`:
  `## YYYY-MM-DD — <task>` then `Done:`, `Next:`, `Blockers:` (one line each).
- Ask before adding any new dependency. Say why it's needed and what it costs (size, maintenance).
- Small commits, Conventional Commits style (`feat(core): …`, `fix(app): …`, `test(…): … (red)`),
  never mixing a refactor with a behaviour change. Decisions go in `docs/decisions/`.
- If you are unsure how a distro tool behaves, say so and propose a fixture test; don't guess.

## Workspace layout

```
crates/core             scanning, sizes, live stats — no UI, no root
crates/adapters         pacman, apt, flatpak, appimage — one module each, behind a trait
crates/helper-protocol  request/response enums shared by app and helper (serde)
crates/helper           tidewise-helper binary: validates and runs allow-listed jobs as root.
                        Ships inside the AppImage; installed once to /usr/local/libexec at setup
                        (root cannot execute it from inside the AppImage's FUSE mount)
crates/app              Slint UI (see docs/decisions/0001-ui-toolkit.md) + view models
packaging/appimage      AppImage build script, .desktop file, icon
packaging/polkit        io.github.tidewise.helper.policy
tests/fixtures          saved real command outputs (pacman, apt, flatpak, /proc)
tests/distro            container smoke tests (Arch, Ubuntu 22.04/24.04, Debian 12)
```

Keep `core` and `adapters` free of any UI or toolkit types. The UI talks to them through view models.

## Commands

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo audit && cargo deny check
cargo build --release -p tidewise-app   # runnable app: ./target/release/tidewise
./packaging/appimage/build.sh        # builds dist/Tidewise-<version>-x86_64.AppImage (from M2)
```

All of these must pass before you say a task is done. Run them; don't assume.

## Definition of done (every task, every session)

1. Tests written first and shown failing, then passing (see Testing rules).
2. fmt, clippy, test, audit and deny all exit 0 — run them **on the final commit**, not earlier.
3. **Build the app last:** `cargo build --release -p tidewise-app`. If a `target/release/tidewise`
   already exists and no file under `crates/`, `assets/`, `Cargo.toml` or `Cargo.lock` is newer than it,
   say it is already up to date instead of rebuilding.
4. **End your final message with a "Run it" block** so the human can try it straight away:
   ```
   Build: up to date | rebuilt   (target/release/tidewise, <date time>)
   Run:   ./target/release/tidewise              # live data
          ./target/release/tidewise --demo       # the design's sample data
          TIDEWISE_RENDERER=gpu ./target/release/tidewise   # GPU renderer
   ```
   Once the AppImage script exists, also give its path and `chmod +x … && ./Tidewise-….AppImage`.
   If the build failed, say so plainly with the error; never claim a build you didn't make.
5. `docs/progress.md` entry written; any rule you bent is listed there with the reason.

## Rust conventions

- Edition 2021, stable toolchain. No `unsafe` without a comment explaining why and the human's OK.
- No `unwrap()`/`expect()` outside tests and `main` setup. Libraries return `Result` with `thiserror`
  error types; the app may use `anyhow` at the top level.
- Parse other programs' **machine-readable** output and force `LC_ALL=C`:
  - pacman: `pacman -Qq`, `pacman -Qi <name>`, orphans `pacman -Qdtq`, dry-run `pacman -Rns --print`
  - apt/dpkg: `dpkg-query -W -f='${Package}\t${Installed-Size}\t${Status}\n'`, dry-run `apt-get -s remove`
  - dnf/rpm: `rpm -qa --qf '%{NAME}\t%{SIZE}\n'`, dry-run `dnf remove --assumeno`, cache `dnf clean packages`
  - snap: `snap list --all` (disabled revisions = old copies), remove `snap remove <name> [--revision=<r>]`
  - flatpak: `flatpak list --app --columns=application,name,size,installation`
  - AppImage: files ending in `.AppImage` in `~/Applications`, `~/.local/bin`, `~/Downloads`
  Every parser gets fixture tests from real saved output, including a non-English and a broken case.
- Sizes: count each inode once (hard links), don't follow symlinks, skip other filesystems by default.
- Live stats refresh at most once per second and pause when the window is hidden.

## Design rules

- Match the screens in `docs/design/` exactly: spacing, copy, states. Reference pictures are in
  `docs/design/png/`. Prove it with the loop in `docs/design/DESIGN-CHECK.md`
  (`tools/design_diff.py`, target LAYOUT MISMATCH ≤ 10 %), never by eye alone.
- Fonts: Geist and Geist Mono from `assets/fonts/`. Icons: `docs/design/icons/`.
- Use only the tokens in `docs/design/tokens.md`. No new colours, no shadows, blur, glow or gradients.
- Every number in mono font; every destructive button names what it removes ("Uninstall GIMP").
- Plain-language label first; technical detail only behind the "Show technical details" switch.
- Every screen needs its empty, loading and error state (see artboard "F · Empty, error & edge states").
- Accessibility: real buttons and labels, keyboard reachable, 4.5:1 text contrast, honour reduced motion.

## Testing rules — this project is test-driven

- **Red → green → refactor.** For every change: write the test first, run it and show it failing,
  write the smallest code that makes it pass, then clean up with the tests still green.
- **Make it auditable:** commit the failing test on its own (`test(scope): … (red)`), then the code
  (`feat|fix(scope): …`). "Written together with the code" is not test-driven; if it happens, say so.
- Keep the test pyramid: mostly unit tests, fewer integration tests, few end-to-end (ENGINEERING.md §1).
- Unit tests: parsers (from fixtures), size maths, view-model logic, the helper's validation.
- Integration tests: real commands in containers (Arch, Ubuntu 22.04, Ubuntu 24.04, Debian 12, Fedora),
  the helper end to end as root inside a container, the AppImage launching on Ubuntu 22.04 and newest.
- No adapter or helper change without a test that fails before and passes after.
- Helper tests include a table of hostile inputs (`--force`, `../`, `*`, empty, very long lists).
- Never run real destructive commands in tests on the host. Use fixtures or the distro containers.
- **Security tests** for anything that parses outside input or runs as root: hostile-input tables,
  `cargo fuzz` targets for every parser, privilege and symlink tests in containers, and an adversarial
  review by a fresh subagent before helper work is done. Keep `docs/security/threat-model.md` current.
  Details and the pre-release human pen-test checklist: ENGINEERING.md §2.
- Never weaken, skip or delete a failing test to get green. A flaky test is a bug.
- Budgets are tests too: idle memory ≤ 40 MB, idle CPU < 2 % on Home, design match ≤ 10 %.

## Things to stop and ask about

- Anything touching `crates/helper`, `crates/helper-protocol` or `packaging/polkit`
- New dependencies, new network calls, new file locations written outside `~/.local/share/tidewise`
- Changing scope: v1.0 is every designed screen (see `docs/PLAN.md`); anything not on that list waits
