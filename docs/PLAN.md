# Tidewise — Plan (rebuild from zero)

Status: **APPROVED v2** (8 Nov 2026).
**Source of truth:** this file in the repo (`docs/PLAN.md`). The claude.ai project copy is refreshed from it (D10).
When a decision changes: edit the decision table, add a line to the changelog at the bottom.

## 1. What Tidewise is

An open-source "PC manager" **for Arch Linux**, for people who are new to Arch or prefer a GUI.
One app to: install apps, update the system, remove apps, clean junk, see what uses disk space, and watch
performance — in plain language, with technical details one toggle away.

**USP:** all-in-one + plain language + safety-first. Arch already has GUI package managers (pamac,
Octopi, KDE Discover / GNOME Software via PackageKit); the installer alone is not the selling point.

**Honest safety goal** (replaces "perfect"): Tidewise never does anything riskier than the correct
CLI command would, always shows the exact change before making it, and refuses known-dangerous patterns.

## 2. Decisions (locked)

| # | Decision | Why |
|---|---|---|
| D1 | Arch Linux only (for now) | Focus; one package manager (pacman) done properly |
| D2 | Ship as an **AUR package (PKGBUILD)**, not AppImage | pacman installs the helper and polkit policy the normal way and removes them cleanly |
| D3 | Installer v1 = **official repos only**; AUR later | AUR builds run untrusted PKGBUILD code; needs its own milestone and review screen |
| D4 | **Rust + Slint**, "Hybrid" design (Apple-like calm grouping + datasheet numbers, light-first, one accent, IBM Plex, red = irreversible, teal = reversible) | Already prototyped and chosen; see `docs/decisions/0001`, `0002` |
| D5 | Test-driven: red → green → refactor, small commits | Understanding + safety |
| D6 | Small steps: read-only features before anything that needs root | Learn the codebase before the risky parts |
| D7 | Same GitHub repo `Cyber-Programmer/tidewise`, **fresh history**; old code kept as tag `archive/old-v1` | Keeps the name/URL; old code doesn't confuse the rebuild |
| D8 | **First public release v0.1 after M4** (Apps + Install + Update system), then grow in v0.x releases | Real feedback early; small steps |
| D9 | **"Update system" screen is in scope** (full `pacman -Syu`) | Installs sometimes need a full upgrade (rule 4); users need a safe way to do it |
| D10 | **Repo docs are the source of truth**; the claude.ai project copies are refreshed from them | Rules must match the code they govern; the repo is what's public |
| D11 | **Read pacman data by parsing CLI output** (`LC_ALL=C`, no shell), not libalpm / the `alpm` crate | Spike 4.1: same results, but libalpm fails silently on bad DBs, panics on non-UTF-8, locks us to libalpm's major version and brings GPL-3.0-only crates; see `docs/decisions/0004` |

## 3. Golden rules (safety)

1. **The app never runs as root.** Only `tidewise-helper` runs as root, started via `pkexec` with one JSON request.
2. **The helper only does allow-listed jobs.** Adding or widening one needs the human's explicit OK.
3. **No shell, ever.** `std::process::Command` with absolute paths and argument lists; helper clears env, sets `LC_ALL=C`.
4. **No partial upgrades.** Never `pacman -Sy <pkg>`, never `-Sy` then `-S`. Install either against the current
   database or as a full `-Syu <pkgs>`. (Arch wiki: System maintenance → "Partial upgrades are unsupported")
5. **Show before change.** Every change has a dry-run producing the exact list on the confirm screen; the helper re-checks that list before acting.
6. **One password prompt per confirmed transaction.** Batch everything into one helper call.
7. **Never delete user files.** User files go to Trash; only caches/logs/package caches are removed directly, after a confirm screen.
8. **Protected things get no destructive button:** `base`, kernel(s), bootloader, desktop session, audio, network, pacman itself, the journal. Refused in code, not only in the UI.
9. **Never touch pacman's lock file** (`/var/lib/pacman/db.lck`). Wait and explain instead.
10. **Never use `--overwrite` or `-d`/`-dd` (skip dependency checks) on user-chosen input.** `--noconfirm` only for a
    transaction the helper has verified: providers named explicitly, a `--print` dry run equal to the confirmed list,
    no removals. (`docs/decisions/0005-helper-invocation.md`, `docs/security/threat-model.md`)
11. **No network** except what pacman itself does, plus an optional update check. No telemetry.
12. AUR (later): build as the normal user, never root; show the PKGBUILD before building.
13. Before a system update, show Arch news the user hasn't seen (manual-intervention notices). *(How: decide in M4; network rule 11 applies.)*

## 4. Scope

**v0.1 (first public release, after M4):** Apps list, Search + details, Install (official repos), Update system.

**v1.0 features (built in this order):**
1. Apps — installed packages list (explicit vs dependencies, sizes)
2. Search + Install — official repos, package details, provider choice
3. Update system — full `-Syu` with preview
4. Uninstall — with dependency preview, orphans
5. Cleaner — pacman cache, user caches, orphans
6. Storage — what uses disk space
7. Performance — CPU/RAM/disk/network live, process list
8. Home + Settings

**Not in v1.0:** AUR, Flatpak, Snap, AppImage management, other distros, scheduled tasks.

## 5. Install flow (design target for the first screens)

```
Search box → Results (name, repo, version, size, "Installed" badge)
   → Package details (description, size, dependencies, technical toggle)
   → [Install <name>]
   → Dry-run: resolve dependencies
        ├─ several providers?  → "Choose provider" dialog   (e.g. java-runtime)
        ├─ a group?            → "Choose members" dialog    (e.g. plasma)
        └─ database stale / package file missing → offer "Update system and install" (-Syu <pkgs>)
   → Confirm screen: exact packages, download size, installed size
   → ONE password prompt (polkit)
   → Helper re-checks the list → pacman runs → live progress
   → Done / clear error message (lock held, no space, conflict, network)
```

Official Arch repos hold one version per package, so pacman asks about **providers/group members**, not versions.

## 6. Architecture (target)

```
crates/core             scanning, sizes, live stats — no UI, no root
crates/pacman           read pacman data (installed, search, info, dry-runs) — no root
crates/helper-protocol  request/response types shared by app and helper
crates/helper           tidewise-helper: validates + runs allow-listed jobs as root (#![forbid(unsafe_code)])
crates/app              Slint UI + view models (binary: `tidewise`)
packaging/aur           PKGBUILD
packaging/polkit        io.github.tidewise.helper.policy
tests/fixtures          saved real pacman output
tests/container         Arch container tests (anything that runs pacman for real)
```

Decided (D11): read pacman data by **parsing CLI output** (`pacman -Qi`, `-Si`, `-Ss`, `-S --print`), not with
libalpm bindings (the `alpm` crate). The M2 spike measured both; see `docs/decisions/0004-pacman-data-source.md`.

## 7. Milestones (small steps; each ends with something you can run and understand)

| M | Name | You'll learn | Needs root? |
|---|---|---|---|
| M0 | Foundations: repo reset (fresh history), workspace, CI, empty Slint window, CLAUDE.md | cargo workspaces, Slint basics, CI | No |
| M1 | Apps list (read-only): installed packages, sizes | parsing pacman output, fixtures, view models | No |
| M2 | Search + package details (read-only); design the Install screens; CLI-vs-libalpm spike | search, async UI, design check | No |
| M3 | Helper + polkit + PKGBUILD skeleton, threat model | privilege separation, polkit | Yes (first time) |
| M4 | Install (providers, dry-run, confirm, one prompt, progress) + Update system → **release v0.1** | transactions, error states, releasing to AUR | Yes |
| M5 | Uninstall + orphans | dependency safety, protected packages | Yes |
| M6 | Cleaner | caches, Trash spec | Yes |
| M7 | Storage | disk scanning, hard links | No |
| M8 | Performance + processes | /proc, refresh budgets | No |
| M9 | Home, Settings, accessibility, polish | — | — |
| M10 | v1.0: security review, pen-test checklist, AUR release | release process | — |

**M3 order (changed 13 Nov 2026, human decision):** 5.1 threat model -> **5.7 container harness (rootless Podman)**
-> the other M3 tasks, so every helper task can be tested against real pacman in a container from the start.
Threat model: `docs/security/threat-model.md`.

Design is done **just in time**: each milestone designs its own screens first.

## 8. Working rules (for Claude Code and chats)

- One task per session; plan first, wait for approval; tests first (show red), then code.
- Explain each change in plain words so the human understands it (the point of the rebuild).
- Ask before new dependencies, anything in `crates/helper*` or `packaging/polkit`.
- After each task: append to `docs/progress.md` and update the WBS sheet (rules in §9).

## 9. Tracking

- WBS sheet: https://docs.google.com/spreadsheets/d/1gRI9jVQAm3w0y9UL2aFld0kT-kqiUSD9oMUV5yTX3Gc
  — tab **WBS** (current plan), tabs **Archive (old plan)** / **Archive (old sources)** (history, never edit).
- Find rows by WBS ID in column A. Status only: To Do, In Progress, Done, Needs Verification.
  Done only with evidence in column L (commit SHAs + test names). Dates like `8 Nov 2026`, only when they happened.
- New work: next free ID under its milestone; never edit column O (Roll-up).

## Changelog
- 8 Nov 2026 — v2 approved: Arch-only rebuild, AUR packaging, repo-only installer, same repo fresh history, v0.1 after M4, Update system in scope.
- 9 Nov 2026 — D10: repo docs become the source of truth (2.5). D7 notes the `archive/old-v1` tag. Rust edition 2024 (not 2021).
- 12 Nov 2026 — D11: CLI parsing for sync data (spike 4.1, decision 0004); the open technical choice in section 6 is closed.
- 13 Nov 2026 — M3 order: 5.7 container harness (rootless Podman) right after 5.1 threat model (human decision).
- 13 Nov 2026 — Golden rule 10 reworded (human decision, option A): `--noconfirm` is allowed only for a transaction the
  helper has verified (providers explicit, `--print` dry run equal to the confirmed list, no removals). Helper
  invocation: pkexec + checked summary in argv (decision 0005).
