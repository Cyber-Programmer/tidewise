# M1 — Core + live screens (≈25 hours)

## Context
Toolkit is decided (`docs/decisions/0001-ui-toolkit.md`). Read `CLAUDE.md`. This milestone is
**read-only**: nothing is changed on the user's system.

## Goal
A real app shell with the sidebar and three working screens with live data: Home (live tiles only),
Performance, Processes (view only, no End task yet).

## Tasks (one per session, in order)
1. Repo hygiene: README (what/why/screenshot placeholder), GPL-3.0 licence file, CI workflow running
   fmt, clippy, test, audit, deny on every push.
2. `crates/app` shell: window, sidebar with all v1 items (Home, Clean up, Apps, Storage, Performance,
   Processes, Settings), tokens wired as theme constants, dark theme only for now.
3. Performance screen: metric list + big chart + plain-language explanation + "Show technical details"
   switch revealing the facts grid (match design 5).
4. Processes screen: group processes by app (parent/`.desktop` mapping, fall back to name), Apps /
   Background / All filter, search, details switch shows PID/user. Mark protected processes
   (session, audio, network) with the lock icon. Unit tests for grouping.
5. Home screen: the four "Right now" tiles from live data; suggestions area shows a placeholder until M2.
6. Distro detection in `core`: OS name, desktop, which package managers and app sources exist. Shown in
   the sidebar card. Fixture tests for `/etc/os-release` variants (Arch, Ubuntu, Debian, Mint, Manjaro).
7. Performance pass: confirm the app idles under the memory measured in the spike; stats pause when the
   window is hidden.

## Acceptance
- Matches artboards 1 (tiles only), 5 and 6 in dark theme.
- Idle memory within 10% of the spike number; no CPU use while hidden.
- Works on Arch and Ubuntu 24.04 (manual check, noted in progress.md).
