# 0004 — How `tidewise-pacman` reads sync data: CLI parsing or libalpm

- Status: **Accepted** (option A, CLI parsing)
- Date: 2026-11-12
- Decided by: Bahauddin Hassan (human), 2026-11-12
- Task: WBS 4.1 (spike). Prototype code: branch `spike/cli-vs-alpm` (throwaway, never merged; deleted after 4.3/4.4).

## Context

M2 adds search and package details for packages that are **not installed yet**, so the data comes from the sync
databases (`core`, `extra`, `multilib`). `docs/PLAN.md` §6 left one choice open: run `pacman` and parse its text
(**A**), or read the databases through libalpm with the `alpm` crate (**B**). Installed packages already use A
(`pacman -Qi`, `-Qq`).

Both prototypes did the same two things, read-only, as a normal user, on this machine
(pacman `7.1.0.r9.g54d9411-2`, libalpm `16.0.1`, sync DBs from 2026-10-14, no network): search `linux`
(name + description + "installed" flag) and details for 20 packages (version, repo, download size, installed size,
depends, provides). Prototype A follows `run.rs`: `/usr/bin/pacman`, argument list, `env_clear`, `LC_ALL=C`, null
stdin, no shell. Prototype B uses `alpm` `=5.0.2` and makes no transaction call.

Spike commits on `spike/cli-vs-alpm`: `a821c4b` (skeleton), `e4a2f7f` (A), `95eaf56` (B), `32b08c2` (`--time`),
`5a19668` (`--pacman`, `is_valid` experiment), `5d9e63c` (unedited copy of `deny.toml`).

## Options

| Option | In one line |
|---|---|
| **A. CLI** | Run `pacman -Ss` / `-Si` (`LC_ALL=C`), parse the text. Same pattern as `installed_packages()`. |
| **B. libalpm** | `alpm` crate (safe wrapper over `alpm-sys`) reads the sync DBs in our own process. |

## Results

Numbers come from commands run for this spike. Where I could not verify something it says "I don't know".

| # | Question | A: CLI | B: libalpm (`alpm` 5.0.2) |
|---|---|---|---|
| 1 | Same results? | Search: **349 / 349** results identical to B (repo, name, version, installed flag including the `[installed: other-version]` form, description). Details: **20 / 20** identical in repo, version, depends, provides. | (reference) |
| 1b | Sizes | Only pacman's rounded text (`358.23 KiB`, `1.67 GiB`). pacman picks the unit itself, and not only from the size (installed size is `6.94 MiB` for zsh but `9820.16 KiB` for bash); I did not work out its rule. All 40 size cells are within pacman's rounding of B's exact bytes (worst error 5 115 bytes). | Exact bytes (`2396`). |
| 2 | Speed (whole binary, warm, 1 warm-up + 5 runs, median; 3 rounds) | **630 / 824 / 695 ms**. Two pacman calls, about **330 ms each** (`-Ss` 334 ms, `-Si` x20 328 ms). pacman's own start-up is 11 ms, so the time is pacman loading `extra.db`. | **308 / 382 / 331 ms**. Open + register 2–5 ms; the first search triggers the lazy DB load (290–570 ms); the 20 detail lookups then take 0.3–0.5 ms in total. |
| 2b | Memory | Short-lived pacman processes; app memory unaffected (not measured). | **≈ 61.5 MB RSS** after the three repos are loaded (RSS, not PSS, so a little high). Budget in `ENGINEERING.md` §5: 40 MB idle PSS. Keep the handle and we are over; drop it after each query and every query pays the load again (≈ A's cost). |
| 3 | Our code (non-blank, non-comment lines, at commit `95eaf56`, before any timing or experiment flags) | **97**, of which **51** are the two parsers (`-Ss` 18, `-Si` 33). | **62**. No parsers. (Both share a 73-line `lib.rs` for arguments and output format.) With `--time` and the `is_valid` experiment B is 82. |
| 3b | `unsafe` | **0** in our code. | **0** in our code. Inside `alpm` 5.0.2: 556 `unsafe {}` blocks, 60 `unsafe fn`, 99 `unsafe impl`, 4 `unsafe trait` (≈ 5 800 lines); `alpm-sys` 5.0.1 has 476 `unsafe` lines (≈ 5 700 lines). |
| 4a | DB never synced (no `sync/`, or empty) | pacman warns `database file for 'core' does not exist (use '-Sy' to download)`, exit 1. We can show it. | **Exit 0, zero results, no error.** Indistinguishable from "nothing matches". |
| 4b | Corrupt DB (4 KiB of random bytes as `core.db`) | `error: could not open file …/core.db: Unrecognized archive format`, exit 9. | **Exit 0, 309 lines; the core repo is silently missing.** |
| 4c | Truncated `extra.db` | `error: could not parse package description file 'alpm-docs-0.12.0-1/desc' from db 'extra'`, exit 11. | **Exit 0, 67 lines**; most of `extra` silently missing. |
| 4d | `Db::is_valid()` as a guard | n/a | Returned `Ok` for the missing, garbage and truncated DBs. My prototype set no signature level (a real B would read `SigLevel` from `pacman.conf`), so this is not a final verdict, but it did not catch them. |
| 4e | Invalid UTF-8 byte in a description | `-Ss` and `-Si` print it; `String::from_utf8_lossy` turns it into U+FFFD (what `run.rs` already does). Exit 0. | **Panic**, exit 101, at `alpm-5.0.2/src/utils.rs:10`: `CStr::to_str().unwrap()` in `from_cstr`, which every string getter (name, version, desc…) uses. Our own crates forbid `unwrap`/`panic!` in library code; a dependency can still bring them. |
| 4f | pacman / libalpm missing | `/nonexistent/pacman` → `cannot start …: No such file or directory`, exit 1 (the `NotFound` error already in `run.rs`). | Simulated soname bump (`patchelf --replace-needed libalpm.so.16 libalpm.so.17` on a copy): **the binary does not start**: `error while loading shared libraries: libalpm.so.17: cannot open shared object file`, exit 127. |
| 4g | Other edges seen in A | `-Ss` with **no match exits 1 with no output**, so the strict "non-zero = error" rule of `check_exit` would turn "no results" into an error. `-Si` with one unknown name among known ones still prints the known ones and exits 1. Exit codes seen: 1, 9, 11, 20. | A missing `dbpath` gives `could not find or read directory` (no path in the message). |
| 4h | Lock file `db.lck` | Not touched. | **Not created and not needed for reading.** `inotifywait` on `/var/lib/pacman` (not writable for the user) while A and B ran: 19 events, all `OPEN`, none for `db.lck`. On a writable temp copy: 0 create/delete events. (`libalpm_handle(3)` says `alpm_initialize` "creates lockfile"; that sentence does not match libalpm 16.0.1 here. I could not read the C source: gitlab blocks automated fetches.) |
| 5 | Build and packaging | Nothing extra. NEEDED: libc, libgcc_s only. | Clean release build 4.9 s. Needs `pkg-config` and `libalpm.pc` + `alpm.h`, both **shipped by the `pacman` package itself** (`pacman -Qo`), so no `-devel` package. No `bindgen`/clang: `alpm-sys` ships pre-generated bindings (`ffi_generated.rs`); the lock file has only `alpm`, `alpm-sys`, `bitflags`, `pkg-config`. Binary NEEDED: **`libalpm.so.16`**. `alpm`'s `build.rs` also asserts libalpm major == 16 and `pkg-config` wants ≥ 16.0.0. |
| 5b | PKGBUILD dependency | none | pacman here `Provides: libalpm.so=16-64`. A hand-written dependency would be on `libalpm.so` (unversioned, or `libalpm.so>=16` as the AUR package `libpamac-aur` did); a versioned one makes pacman refuse an update that breaks it. The soname man page ([alpm-soname(7)](https://man.archlinux.org/man/alpm-soname.7)) says makepkg can also derive `lib:libalpm.so.16` from the binary's NEEDED field when `autodeps` is on. I don't know which hand-written form the Arch guidelines prefer: I found no page that states it. |
| 6 | Rebuild risk | None for the soname. The text format is the risk (see 6b). | See "Rebuild risk" below. |
| 7 | Licences, `cargo deny` | No new crate. | `alpm` 5.0.2 and `alpm-sys` 5.0.1 are `GPL-3.0` (deprecated SPDX id for GPL-3.0-only). `cargo deny check` in `spike/` with an unedited copy of the root `deny.toml`: **advisories ok, bans ok, sources ok, licences FAILED** (`GPL-3.0` "not explicitly allowed", both crates). Fix would be one allow-list entry: needs the human's OK, not done. |
| 8 | Reading `pacman.conf` | **Free.** pacman reads its own config (repos, `Include`, `DBPath`, `SigLevel`, `Architecture`). | **Not free.** The spike hard-coded `core/extra/multilib` and `/var/lib/pacman`. See "What a real B must also read" below. |

### Rebuild risk (point 6)

Sources first, then what is inference.

- **Soname history I could verify.** pacman 5.1.0 moved libalpm from `.10` to `.11` ([Arch todo: pacman 5.1.0 rebuild](https://archlinux.org/todo/pacman-510-rebuild/)). In September 2024 users of pacman `7.0.0.r3.g7736133-1` (in the [Arch Linux Archive](https://archive.archlinux.org/packages/p/pacman/) on 2024-09-09) got `yay: error while loading shared libraries: libalpm.so.14: cannot open shared object file`, fixed by rebuilding yay ([EndeavourOS forum, 2024-09-14](https://forum.endeavouros.com/t/libalpm-so-14-error-after-updating-to-pacman-7/60333?page=2)). In late 2025 pacman 7.1.0 moved `.15` to `.16`: `breaks dependency 'libalpm.so=15' required by libpamac-aur`, and paru failed with `undefined symbol: alpm_option_set_disable_sandbox` ([forum, 2025-12-13](https://forum.endeavouros.com/t/solved-latest-pacman-update-breaks-aur-and-yay/76959?page=3), paru issue opened 2025-12-04). So **two bumps I can verify in about 15 months** (Sep 2024 to Dec 2025), none that I found since. This machine's `-2` build has libalpm 16.0.1; I did not check the later `-3`/`-4` builds that the Archive lists (2026-10-15 and 2026-10-24), so "no bump in the last 11 months" is unverified.
- **I don't know:** the dates and numbers of the bumps between `.11` (2018) and `.14` (2024), and which exact pacman build first shipped `.16` (the Archive shows `7.0.0.r7` and `7.1.0.r1` on the same day, 2025-11-24). I could not fetch pacman's `NEWS` (gitlab returned an access-denied page), so no pacman-side changelog backs this up.
- **Arch ships pacman snapshots** (`7.0.0.rN.gHASH`, `7.1.0.rN.gHASH`), not only tagged releases, as the Archive listing shows. A bump can arrive inside a snapshot.
- **Who rebuilds.** For official packages, Arch maintainers rebuild dependants in staging (the 5.1.0 todo list is an example). An AUR package gets no such rebuild: users hit the failures above until its maintainer rebuilds.
- **How the `alpm` crate tracks pacman.** `alpm` 5.0.2's `build.rs` asserts libalpm major == 16 at build time (read in the crate source). The repository README says "Currently alpm.rs supports libalpm v15.x.x" ([README](https://github.com/archlinux/alpm.rs)), which is stale next to that `build.rs`. The GitHub releases page is empty. From the [crates.io release dates](https://crates.io/api/v1/crates/alpm/versions) the crate major seems to follow the libalpm major: 3.0.x on 2024-03-05/06 (3.0.0–3.0.2 were yanked), 4.0.0 on 2024-08-25 (the day after pacman `7.0.0.r1`), 5.0.0 on 2025-12-05 (11 days after `7.1.0.r1`). **That mapping (3 → 14, 4 → 15, 5 → 16) is my inference from dates and `build.rs`; it is not documented.** `4.0.4` (2025-12-12) came after 5.0.0, so older majors still get fixes.

### What a real B must also read (addition 1: `pacman.conf`)

pacman knows its repos, `DBPath`, `RootDir`, `SigLevel`, `Architecture` from `/etc/pacman.conf` (here: `[options]`, then `[core]`, `[extra]`, `[multilib]`, each with `Include = /etc/pacman.d/mirrorlist`). libalpm does not read that file; the handle must be told. Three ways:

| Way | Cost |
|---|---|
| **Spawn `pacman-conf`** (ships with pacman, `/usr/bin/pacman-conf`; `--repo-list`, `DBPath`, `SigLevel`, …) | One more process (≈ 14 ms) and a small parser for its line output, with fixtures. No new crate. Still a CLI call, so B is no longer "no process". |
| **`pacmanconf` crate 3.1.0** | GPL-3.0, plus `cini` 1.0.0 (GPL-3.0): two more GPL-3.0-only crates. It is only a wrapper that runs `pacman-conf` (looked up in `PATH` unless a `bin` path is given; I saw no `env_clear` in its source), so it breaks golden rule 3 unless we pass the absolute path and accept its environment handling. |
| **Our own parser** | Sections, `Include`, `SigLevel` forms, `Architecture = auto`. Tests, a fuzz target, and a risk of disagreeing with pacman on odd files. The `Server =` lines (with `$repo`/`$arch`) are not needed to read DBs, but I did not check what else an `Include` may legally contain (for example a file that defines more repos). |

**A has no equivalent cost**: it inherits whatever pacman does. The spike's B also left out `SigLevel`, which `is_valid()` (4d) may depend on.

## Recommendation (mine; the decision is yours)

**Choose A (CLI parsing) for v0.1**, with these conditions:

- Treat `-Ss` exit 1 with empty output as "no results", and `-Si` with a known-name subset as partial success; keep pacman's stderr for the technical details.
- Add `parse_ss` / `parse_si` in `crates/pacman` with fixtures from real output (including a broken case, an `[installed: other-version]` line and a description with a stray byte) and two fuzz targets next to `qi` and `qq`. The existing `parse_size`, name checks and `run.rs` already cover the rest.
- Speed is the cost of A (≈ 0.3 s per pacman call, 0.65–0.8 s for search + details). Both options pay the same ≈ 0.3 s DB load per process; B only wins by keeping a handle open, which costs ≈ 61 MB. If it feels slow, one `pacman -Si` for everything is ≈ 0.83 s and 10.8 MB of text (15 522 packages) and could be cached in the app. I did not measure parsing time or the memory of holding it, so this is an idea to test, not a result.

Why: A matches the golden rules and the existing code (no new dependency, no unsafe, no shell, same pattern as `-Qi`), reports every error pacman reports, and survives a libalpm soname bump. B is faster and exact, but in the spike it **fails silently on missing or corrupt databases, panics on a non-UTF-8 byte, locks us to libalpm's major version at build and run time, needs its own `pacman.conf` handling, and is over the memory budget if the handle stays open.** B's advantages (exact sizes, per-name lookup in under a millisecond) do not outweigh that for a v0.1 that must be reliable. Revisit B only if measured latency is a real problem and the four B problems above have an answer.

## Consequences

**If A (recommended):**
- `tidewise-pacman` stays free of third-party dependencies. `deny.toml` unchanged.
- Two more text parsers to maintain, fixtures to keep when pacman's output changes, and fuzz targets to run for 10 minutes before the milestone closes.
- Sizes come as rounded text; fine for the UI, not for anything that needs exact bytes.
- Each query costs a pacman process (≈ 0.3 s); the UI must stay async (it already is for `-Qi`).

**If B:**
- **Licence.** `alpm` and `alpm-sys` are GPL-3.0-only. Linking them makes the combined binary GPL-3.0, not "or later". The app is already GPLv3 because Slint is used under its GPLv3 option ([0001](0001-stack-and-packaging.md), "so the combined app is GPLv3"), so the app's effective licence does not change. What changes: `tidewise-pacman` (no third-party dependency today) would carry a GPL-3.0-only dependency too, and `deny.toml` needs a `GPL-3.0` allow entry (the human's OK). I did not check libalpm's own licence.
- **libalpm major-version lock, at build and at run time.** Build: `alpm`'s `build.rs` fails when libalpm's major is not 16, and a new `alpm` major (and a code change if the API moved) is needed for each libalpm major. Run: the binary's NEEDED is `libalpm.so.16`. After `pacman -Syu` brings `.17`, a Tidewise built against `.16` does not start (exit 127, simulated above), and the `tidewise-helper` would need the same care. A versioned PKGBUILD dependency instead makes pacman refuse the upgrade until the AUR package is rebuilt (seen with `libpamac-aur`). Tidewise is the tool users run to update, so either failure lands on its main job. Two bumps in about 15 months is the verified rate.
- Our own checks for "DB missing", "DB empty" and "repo has zero packages", since libalpm stays silent; a `catch_unwind` or an upfront UTF-8 scan for the panic (not tried); `pacman.conf` handling (table above); a decision on handle lifetime against the 40 MB budget.
- Fewer parsers, exact sizes, and fast repeated lookups.

**Either way:**
- The spike branch is evidence only. After 4.3/4.4 it is deleted; this record keeps the SHAs and numbers.
- `CLAUDE.md` gets `spike/…` in the branch prefix list (same PR as this record).
- Not tested: a libalpm built with a different `DBPath`/`RootDir` than the default, signature-checked databases (`SigLevel = Required`), other repo layouts than this machine's, pacman's behaviour while another pacman holds the lock, and timing on a cold cache or a slow disk.
