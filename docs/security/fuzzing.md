# Fuzzing

Targets live in `fuzz/` (a separate crate, nightly only, not in the workspace). They feed arbitrary UTF-8
to a parser and require only that it does not panic, hang or run out of memory.

```
cargo install cargo-fuzz --locked        # once
cargo +nightly fuzz build
cargo +nightly fuzz run <target> -- -max_total_time=300
```

## Run of 2026-10-28 (run 04), 5 minutes each, 12-core laptop, six targets in parallel

| Target | Parser | Runs | Crashes |
| --- | --- | --- | --- |
| `pacman_qi` | `adapters::pacman::parse_qi` (`pacman -Qi`) | 2 461 178 | 0 |
| `flatpak_list` | `adapters::flatpak::parse_list` | 3 310 206 | 0 |
| `os_release` | `core::distro::parse_os_release` | 2 499 278 | 0 |
| `statm` | `core::statm::parse_statm` | 11 364 379 | 0 |
| `proc_stat` | `core::procfs::parse_stat` (`/proc/<pid>/stat`) | 5 361 107 | 0 |
| `desktop` | `core::distro::parse_desktop` (`XDG_CURRENT_DESKTOP`-style lists) | 7 133 084 | 0 |

No crash, timeout or out-of-memory finding, so no new fixtures or regression tests were needed.
Corpora are not committed (`fuzz/.gitignore`); `.desktop` *file* parsing does not exist yet — the
`desktop` target covers the desktop-name list only. Add a target when that parser is written.

## Run of 2026-11-01 (run 06, R6-Q05), 20 seconds each, short smoke run only

| Target | Parser | Runs | Crashes |
| --- | --- | --- | --- |
| `journal_usage` (new) | `adapters::journal::parse_disk_usage` (`journalctl --disk-usage`) | 696 769 | 0 |
| `flatpak_list` (extended) | `parse_list`, `parse_runtimes`, `parse_app_runtimes`, `unused_runtimes` | 301 318 | 0 |

| `pacman_rns_print` (new, R6-Q09) | `adapters::pacman::parse_removal_print` (both success and failure text) | 851 104 | 0 |
| `apt_parsers` (new, R6-Q10) | `adapters::apt::{parse_dpkg_query, parse_simulated_remove}` | 509 694 | 0 |
| `dnf_parsers` (new, R6-Q11) | `adapters::dnf::{parse_rpm_qa, parse_dry_run}` | 541 726 | 0 |
| `snap_list` (new, R6-Q12) | `adapters::snap::parse_snap_list` | 863 864 | 0 |

20 s is **not** the 10 minutes ENGINEERING.md §2 asks for; the full runs are queue task Q23.

## Follow-ups

- CI does not run fuzzing yet. Suggested: a weekly scheduled workflow, 60 s per target, uploading any crash input.
- Each target should run at least 10 minutes before a milestone closes (ENGINEERING.md §2); this run used 5.
- M3: add targets for the helper's JSON request and the apt/dnf/snap parsers when they exist.
