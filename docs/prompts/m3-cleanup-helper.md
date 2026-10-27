# M3 — Clean up + the admin helper (≈30 hours)

## Context
Read `CLAUDE.md`, especially the golden rules, and the Security model section of the plan.
This is the riskiest milestone: the first code that runs as root. Plan every session first and wait
for approval. The human reviews every line in `crates/helper*` and `packaging/polkit`.

## Goal
Clean up works end to end (design 2 + flow C): scan → review → confirm → one password prompt →
progress → report, with skipped/stopped-early states.

## Tasks
1. `crates/helper-protocol`: request = list of steps, each an enum variant with bounded fields
   (`CleanPkgCache { keep: 1..=5 }`, `VacuumJournal { days: 1..=90 }`, `RemoveOrphans`, ...);
   response = per-step result. serde with `deny_unknown_fields`. Property tests for bounds.
2. `crates/helper` binary: read one JSON request from stdin (size-limited), validate, refuse anything
   not on the allow-list, clear env, `LC_ALL=C`, run fixed tools by absolute path, no shell, structured
   JSON result, log to the journal. Hostile-input test table. Fallback "keep newest N" for pacman when
   `paccache` (package `pacman-contrib`) is missing.
3. Polkit policy `io.github.tidewise.helper.policy` with the `org.freedesktop.policykit.exec.path`
   annotation pointing to `/usr/local/libexec/tidewise-helper`; message names Tidewise. Default
   `auth_admin`.
4. Install/uninstall flow for the helper (Settings → Admin access, and first run): copy helper + policy
   to a private 0700 temp dir, one `pkexec` install step, verify the installed file's hash afterwards,
   show the result. Uninstall via the helper's own `uninstall-self`.
5. Junk scanner (as the user): app caches (skip caches of running apps), trash size, package cache size,
   journal size, unused Flatpak runtimes (`flatpak uninstall --unused` dry-run). Dry-run output becomes
   the confirm list.
6. Clean up screen + flow C UI: confirm dialog, waiting-for-password state, progress with "stop after this
   step", done report, partial/skipped state, and History-ready JSON record in
   `~/.local/state/tidewise/history/`.
7. Container tests: run the helper as root in Arch and Ubuntu containers against real caches; verify
   only expected files disappear.

## Acceptance
- A security self-review checklist (in `docs/security-review-m3.md`) answered item by item.
- Hostile-input tests all refused; container tests green.
- Cancelling the password prompt leaves the system untouched and shows the skipped state.

## Don't
- No helper action that deletes arbitrary paths. User files never pass through the helper.
- No `auth_admin_keep` or broader polkit defaults without asking.
