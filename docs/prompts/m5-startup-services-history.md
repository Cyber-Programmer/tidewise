# M5 — Startup apps, Services, History (≈30 hours)

## Context
Read `CLAUDE.md` (test-driven). Adds the `ServiceControl` helper action: get the human's approval of
its exact shape before coding. Designs: 7 · Startup apps, 8 · Services, 9 · History.

## Tasks
1. Startup apps (as the user, no helper): read XDG autostart entries from `~/.config/autostart` and
   `/etc/xdg/autostart`; turning one off writes a user override with `Hidden=true` (never edits files
   in `/etc`). Keep a backup for Undo. Impact = memory and CPU used in the first minute after login,
   from a lightweight sample taken at login only if the user enabled it; otherwise show "unknown".
   Fixture tests for `.desktop` parsing first.
2. Services list (as the user): `systemctl list-units` / `list-unit-files` with machine-readable
   output; failed units; a curated "worth a look" list (Docker, printing, Bluetooth, failed units)
   with plain explanations. Protected units list (network, journal, display manager, audio, dbus,
   polkit) refused in code.
3. Helper `ServiceControl { unit, action: Start|Stop|Enable|Disable }`: unit must exist, must not be
   protected, name pattern checked. Hostile-input tests first. Integration test in a systemd container.
4. History: every action from M3/M4/M5 writes a JSON record in `~/.local/state/tidewise/history/`;
   History screen lists them by day, shows "what changed", Undo/Reinstall/Restore where possible,
   and "Copy for a help forum" (plain text, no username or file contents).
5. Startup toast with Undo; Services undo via History.

## Acceptance
- Matches artboards 7, 8, 9 including details toggles.
- Protected units and essential autostart items can't be changed (tested).
- Every reversible action from M3–M5 can be undone from History (integration tests).
