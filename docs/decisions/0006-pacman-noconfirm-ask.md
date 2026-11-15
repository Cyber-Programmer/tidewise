# 0006 — How the helper runs pacman: `--noconfirm --ask 66`, output in files, its own process group

- Status: **Accepted** (`--ask 66`, files in `/run/tidewise-helper/`, a process group per pacman; the helper's own signals left as a documented residual)
- Date: 2026-11-15
- Decided by: Bahauddin Hassan (human), 2026-11-15 (the 5.3b plan: `--ask 66`, the post-check notice, the work folder)
- Task: WBS 5.3b; follows golden rule 10 and `docs/security/threat-model.md` (T-04, T-08, T-09, T-10, T-17, T-18, T-22, T-23)

## Context

Golden rule 10 lets the helper pass `--noconfirm` for a transaction it has verified: providers named explicitly, a
`--print` dry run equal to the confirmed list, no removals. Everything below was measured with real pacman 7.1.0 in the
Tidewise test container (no network, throwaway), and is now checked again on every container run
(`tests/container/tests/pacman_prompts.rs`).

1. **`--noconfirm` answers every question with its default, and two defaults are "yes" for things that matter.**
   `-Su --noconfirm` answered "Replace old-pkg with new-pkg? [Y/n]" yes and **removed `old-pkg`**. The dry run lists the
   replacing package and says nothing about the removal, so "no removals" (rule 10) was not true for an update. An
   explicitly named package in `IgnorePkg` is installed (default yes; it is in the confirmed list, so allowed). A PGP key
   import is also a default-yes question.
2. **The dry run does not notice conflicts.** `-S --print` printed the target and exited 0 while the real run aborted
   ("Remove conf-x? [y/N]" defaults to no, "unresolvable package conflicts detected", nothing installed).
3. **The dry run does not need the lock.** With `db.lck` present `-Qu`, `-S --print` and `-Su --print` still worked; only
   `-Sy`, `-S` and `-D` failed ("unable to lock database").
4. **Signals.** A reader of pacman's stdout that goes away kills pacman with SIGPIPE at its next write and leaves a
   stale `db.lck`. SIGHUP in the middle of a commit makes pacman stop after the current package with **exit status 0**:
   a half transaction that looks like success (a partial `-Su` would be a partial upgrade). SIGTERM kills it at once and
   leaves the lock. pacman installs handlers for SIGINT and SIGHUP only (`src/pacman/sighandler.c`), none for SIGTERM
   or SIGPIPE.
5. **umask.** pacman does not fix the umask. Started under umask 077 the downloaded package file in
   `/var/cache/pacman/pkg` came out mode 600, under 000 mode 666 (world-writable). libalpm sets `umask(0022)` only
   while unpacking and for scriptlets and hooks (`lib/libalpm/util.c`: `_alpm_unpack`, `_alpm_run_chroot`).

## Options for the questions pacman asks

| Option | In one line |
|---|---|
| A. `--noconfirm` only | What rule 10 said. Removes a replaced package silently. |
| B. `--noconfirm --ask 2` | Declines replacements. |
| **C. `--noconfirm --ask 66`** | Declines replacements (2) and unattended PGP key import (64). |
| D. Refuse an update that has a pending replacement | Needs `pacman -Si` parsing in the root helper per new package, and still trusts that check to be complete. |

## Decision

**C.** Every real transaction (`-S` and `-Su`) is `pacman <op> --noconfirm --ask 66 -- <names>`; no read and no `-Sy`
carries them (tested in `crates/helper/src/pacman_args.rs`).

`--ask <bitmask>` inverts pacman's default answer for the question types in the mask. The values are the
`alpm_question_type_t` enum in `/usr/include/alpm.h:975-990`: `INSTALL_IGNOREPKG = 1<<0`, `REPLACE_PKG = 1<<1`,
`CONFLICT_PKG = 1<<2`, `CORRUPTED_PKG = 1<<3`, `REMOVE_PKGS = 1<<4`, `SELECT_PROVIDER = 1<<5`, `IMPORT_KEY = 1<<6`, so
66 = 2 + 64. In `src/pacman/callback.c` (`cb_question`, read in Debian's copy of 7.1.0-3) the last thing the function does,
for every question type, is `if(config->noask) { if(config->ask & question->type) { /* inverse the default answer */
question->any.answer = !question->any.answer; } }`. In `--print` mode it first answers `INSTALL_IGNOREPKG` and
`REPLACE_PKG` with yes and everything else with no (which is why the dry run lists replacements and notices no
conflict).

### What was run, and what was not

- **Run (and a permanent container test each):** flag 2 declines the replacement (the old package stays, the other
  upgrades go ahead, exit 0); without the flag the old package is removed. Flag 1 declines the "ignored package, install
  anyway?" question. Flag 4 would **remove** the conflicting package: it is never used. A conflict aborts under
  `--noconfirm` by default.
- **Not run: flag 64.** libalpm looks the key up over the network before it asks whether to import it, and the container
  has no network, so no offline test reaches the question. It rests on the source: the answer field of the key-import
  question (`import_key.import`) is the second member of its struct, at the same offset as `any.answer` in the union
  (`alpm.h:1069-1078`, `1086-1091`), and the inversion above runs for every question type. If the reading were wrong,
  the unattended default would stay "import", as it would be without the flag: the flag cannot make it worse. The
  human accepted this and asked to be told here (approved with the plan).
- **Not fetched:** the code that parses `--ask` (`pacman.c`); the experiments show `--noconfirm --ask N` together behave
  as described.

### Output in files, a process group, and the post-check

- **Files, not pipes** (finding 4): pacman's stdout and stderr go to two root-only files in `/run/tidewise-helper/`
  (created if missing, mode 0700, verified a real directory owned by root and not a symlink; files with `O_EXCL`, mode
  0600, named from the process id and a counter). The helper reads them as they grow (progress) and after the run, and
  removes them. At the start of a job it removes only leftovers of its own name pattern whose process is dead.
- **Own process group** (`CommandExt::process_group(0)`, safe): a terminal's SIGINT or SIGHUP, sent to the helper's
  group, never reaches pacman. Without it the SIGHUP test shows pacman stopping between packages.
- **Post-check:** after every real run `pacman -Q` of each confirmed item; the count that is not installed at the
  confirmed version is reported as a `Notice` just before the final line. A declined replacement, which `--print` keeps
  listing, is the case it exists for.

## Consequences

- **A declined replacement stays pending.** Every later `-Su --print` still lists the new package and the update never
  fully converges until the user does it in a terminal. The M4 UI must explain it (progress `Next`).
- **The helper itself can still be killed** by SIGHUP, SIGINT or SIGTERM sent to it (std cannot ignore signals without
  `unsafe`). pacman, in its own group and writing to files, still finishes and releases its lock; the `-D --asdeps` step
  does not run (the chosen provider stays "explicitly installed", asserted in the container test) and, in an update, a
  kill between `-Sy` and `-Su` leaves the package lists newer than the packages (threat model Q12). Hardening with a
  signal crate is open question 16, not part of this task.
- **Stale lock.** A pacman killed by SIGTERM leaves `db.lck`. The helper reports DatabaseLocked, never removes it (rule
  9), and the app must explain "wait, check nothing is running" (M4).
- **`rustix`** (`std`, `fs`, `process` features) is a direct dependency of the root helper for `umask(022)`; its
  `umask` is gated by the `fs` feature (`src/process/umask.rs:19`).
- Threat model rows T-22 (work folder) and T-23 (`--noconfirm` and `--ask`) are new; T-04, T-08, T-09 to T-15, T-17 and
  T-18 are updated.
