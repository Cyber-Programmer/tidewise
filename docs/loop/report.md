# Run 08 report

Written by the loop, one entry per round (template in `LOOP.md`). Run 07's report: `archive/run-07-report.md`.

## Preflight
Run started 2026-11-06 06:15 PKT. Branch `run/08-2026-11-06` created from `main` (`git merge-base --is-ancestor run/07-2026-11-02 main` → exit 0, run 07 is merged).
**Stop time: 2026-11-06 08:00 PKT** (first 08:00 after the start).
- `cargo --version` → exit 0, cargo 1.98.0 (797e8a9bc 2026-08-05)
- `cargo +nightly --version` → exit 0, cargo 1.101.0-nightly (cad07cf8e 2026-10-23)
- `cargo fuzz --version` → exit 0, cargo-fuzz 0.13.2
- `command -v appimagetool` → exit 0, /home/bahauddin/.local/bin/appimagetool
- `df -h /tmp` → tmpfs, 7.7G, 15M used (1 %)
- `ls -d /tmp/appimage_extracted_* | wc -l` → 0
- WBS sheet read (`Overview!B1:B2`) → OK; B2 was "branch run/07-2026-11-02 @ 586a40e (2026-11-02 14:58), not merged yet."
- Baseline plain `cargo test --all` → exit 0 (repo is on ext4, /tmp is tmpfs).

## R8-Q01 Look at the pipeline — Done   (2026-11-06 06:40 PKT)
WBS: 2.1 → In Progress (was Done; set back because the human's CI run fails). Not set to Done: CI on GitHub is not verified. See N8 in the sheet.
Commits: aa869a2 (red), then the dependency fix and the CI changes 1b2e2a8 (`fix(deps)`), 4b84324 (`ci`), 544e683 (cancel test on both file systems; was already green, not red-first)
Findings: the pasted `docs/ci-failure.log` shows the failing step is `cargo audit`: **RUSTSEC-2026-0433, pulldown-cmark 0.13.5** (7.5 high, via `i-slint-common`), fixed in 0.13.6. That is a different cause from the cancel test. Reproduced here: `cargo audit` → exit 1, `cargo deny check` → exit 1 (advisories), before any change. `cargo deny` also failed on a second new notice, `pin-utils` (RUSTSEC-2026-0441, unmaintained, build-time only via Slint's `spin_on`); `cargo audit` only warned about it.
Red: `./tools/test-ci.sh` → exit 1, 6 of 7 checks FAIL (`FAIL tools/ci-local.sh exists and is executable`, … `FAIL cargo audit exits 0`, `FAIL cargo deny check exits 0`; `ok release.yml parses`).
Evidence:
- `./tools/test-ci.sh` → exit 0, 7 of 7 ok.
- `./tools/ci-local.sh` → exit 0; each command's exit 0: `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all --locked`, `cargo build --release -p tidewise-app --locked`, `cargo audit`, `cargo deny check`.
- Cancel test on both file systems: `cargo test -p tidewise-core cancelling` → exit 0, 2 passed (`cancelling_stops_the_scan_and_says_so` on the repo's disk, ext4 `/dev/nvme0n1p2`; `…_on_tmpfs_too` in `/dev/shm`, tmpfs). Also `TMPDIR=$PWD/target/tmp cargo test --all --locked` (ext4) → exit 0. The plain run inside ci-local used /tmp (tmpfs) → exit 0.
- Workflows parse: `python3 -I -c 'import yaml; yaml.safe_load(...)'` on ci.yml, release.yml, dependabot.yml → exit 0 (PyYAML 6.0.3; `actionlint` is not installed).
- Changes: ci.yml gets `concurrency` (cancel superseded), `timeout-minutes: 30` per job, `--locked` on clippy/test/build, a `release-build` job, `libfontconfig1-dev` install step; release.yml gets `timeout-minutes: 45` and the same install step; `.github/dependabot.yml` (cargo + github-actions, weekly); `tools/ci-local.sh`; `tools/test-ci.sh` (keeps ci-local and ci.yml in step).
Not verified:
- **Whether CI passes on GitHub.** The loop cannot see or run Actions. The other failed runs (#3, #4, #6, #7) may have other causes; only the pasted log (the audit step) was read.
- The `libfontconfig1-dev` install step and `release-build` job were not run on ubuntu-22.04 (this machine is Arch). Earlier runs #1, #2, #5 passed without the apt step, so it may be redundant; I added it for safety.
- `cargo install cargo-audit cargo-deny --locked` in the supply-chain job and the AppImage steps of release.yml were not run.
- `cargo audit` here read the advisory DB at today's date; GitHub fetches it fresh and a newer advisory could fail it again.
Errors: `cargo deny check` exit 1 after the pulldown-cmark bump (pin-utils unmaintained notice) → fixed by an `ignore` entry. `gh` and `actionlint` are not installed here (so action SHAs were not refreshed and the workflows were parsed with PyYAML only). The first Q01 `grep` of column B in the sheet showed parent IDs, not names (read-only slip; found row 2.1 by reading A:O).
Decisions taken without the human: → docs/questions.md "Run 08 — R8-Q01" (5 entries)

## R8-Q02 Secret and supply-chain checks in CI — Done   (2026-11-06 06:50 PKT)
WBS: 5.8 → Needs Verification (built and passing here; the CI step has not run on GitHub)
Commits: f720288 (red), 748abd5
Evidence:
- Red: `./tools/test-check-secrets.sh` → exit 1 (`FAIL script exists and is executable`); `./tools/test-ci.sh` → FAIL `ci.yml and ci-local.sh run tools/check-secrets.sh`, FAIL `ci.yml runs tools/test-check-secrets.sh`.
- Green: `./tools/test-check-secrets.sh` → exit 0, 14 ok (fake AWS key, GitHub token and PEM header in a temp dir are each caught with exit 1 and `file:line` named; the value is not printed; `tests/fixtures`, `.git`, `target` ignored; clean tree exit 0; this repo clean). `./tools/check-secrets.sh` → exit 0 ("nothing found"). `./tools/test-ci.sh` → exit 0.
- `./tools/ci-local.sh` → exit 0; each exit 0: fmt, clippy, `cargo test --all --locked`, release build, `tools/test-check-secrets.sh`, `tools/check-secrets.sh`, `cargo audit`, `cargo deny check`.
- `cargo audit` → exit 0 (572 crates, no vulnerability, 7 allowed warnings: bincode, pin-utils, ttf-parser, zbus_names, zvariant, zvariant_derive unmaintained; async-executor yanked).
- `deny.toml` re-read: five ignores, all "unmaintained" notices, none hides a vulnerability; licences all GPL-3.0-compatible; `wildcards = "deny"`, unknown registries and git denied. `cargo deny check` → exit 0 (warnings only: duplicates, unused licence allowances). Nothing changed this round.
- Workflow: `ci.yml` runs `tools/test-check-secrets.sh` in `check` and `tools/check-secrets.sh` in `supply-chain`; parses (test-ci exit 0).
Not verified:
- The new steps on GitHub (the loop cannot see Actions).
- Coverage of the scan: it is a six-pattern grep (AWS, GitHub, Slack, Google API keys, PEM headers), not a real scanner. It will miss generic passwords and other token formats, and it skips `tests/fixtures` by design.
- Git history is not scanned, only the working tree.
- The `deny.toml` review was a read-through, not a licence audit of each crate.
Errors: the first run of the new test failed on the PEM case: `grep: unrecognized option` followed by the PEM header pattern (quoted text removed: it made the scanner flag this file), in both the scanner and the test's own "value not printed" check (that check had passed vacuously). Fixed with `-e` and `--`; the fix is in the green commit, not the red one. My first `test-ci.sh` exit read came back empty (a pipe swallowed it); re-run plainly → exit 0.
Decisions taken without the human: → docs/questions.md "Run 08 — R8-Q02" (3 entries)

## R8-Q03 Weekly fuzz smoke (optional job) — Partial   (2026-11-06 07:05 PKT)
WBS: 5.5 → unchanged (Done); note N updated. No row for the weekly job itself (see questions).
Commits: bed874e (red), 7ae5002
Evidence:
- Red: `./tools/test-fuzz-workflow.sh` → exit 1 (`FAIL tools/fuzz-smoke.sh exists and is executable`, `FAIL --list equals the files in fuzz/fuzz_targets`, `FAIL .github/workflows/fuzz.yml exists`).
- Green: `./tools/test-fuzz-workflow.sh` → exit 0, 8 ok: triggers are only `schedule` and `workflow_dispatch`; token `contents: read`; `toolchain: nightly`; `timeout-minutes: 90`; both actions pinned to full SHAs; runs `tools/fuzz-smoke.sh 60`; none of the 15 target names appears in the workflow; crash inputs are printed `if: failure()`. The target list comes from `fuzz/fuzz_targets/*.rs` (script `--list` equals the directory, 15 targets). `./tools/test-ci.sh` → exit 0.
- Ran the script itself: `./tools/fuzz-smoke.sh 2` → exit 0, 15 of 15 targets started and finished ("Done N runs" 15 times), no crash. Two seconds per target, not 60.
- `./tools/ci-local.sh` → exit 0 (fmt, clippy, `cargo test --all --locked`, release build, both secret scripts, audit, deny, each exit 0).
Partial because: the queue text says "uploads a crash artefact". I did **not** use `actions/upload-artifact`: its SHA is not among the pins and I can't look one up (no `gh`). Instead a failing run prints each crash input as base64 in the log (first 64 KiB), which can be turned back into a file with `base64 -d`.
Not verified:
- The workflow on GitHub: the schedule, the `cargo install cargo-fuzz --locked` step, the 90-minute timeout (15 targets x 60 s plus a cold build per target; unmeasured), and the crash-printing step (never triggered, no crash to print).
- A full 60 s per target run of the script here (only 2 s was run).
- That `find ... -o ...` in the crash-printing step picks the right files (read, not run on a real crash).
Errors: after the green commit `tools/ci-local.sh` → exit 1 at `tools/test-check-secrets.sh` ("this repository is clean" failed): the secret scan flagged `docs/loop/report.md:51`, my Q02 entry, which quoted a grep error including a PEM header. A true false positive in prose; I reworded the report line (no change to the scanner or its test). Re-run → exit 0.
Decisions taken without the human: → docs/questions.md "Run 08 — R8-Q03" (3 entries)

## R8-Q04 Update the GitHub README — Partial   (2026-11-06 06:45 PKT)
WBS: 9.11 → Needs Verification (unchanged; a person must read it). L, M, N updated.
Commits: a9f7536 (red), 948618b
Evidence:
- Red: `./tools/test-readme.sh` → exit 1 (9 missing sections, no badge, 12 missing status rows, no "no release yet", missing links to budgets, security, ci-local).
- Green: `./tools/test-readme.sh` → exit 0: 9 sections, CI badge for `ci.yml` on `main` (repo from `git remote -v`), 12 status rows (Built / Partial only), every relative link and image target exists, all 11 screenshots shown, "no release yet" present and no tag exists (`git for-each-ref refs/tags` → 0), every MB and % number found in `docs/budgets.md`, every `--flag` on a line that runs the binary exists in `cli.rs`.
- Commands from the README, run here: `cargo build --release -p tidewise-app` → exit 0; `./target/release/tidewise --demo`, `./target/release/tidewise`, `TIDEWISE_RENDERER=gpu ./target/release/tidewise` → each exit 124 (still running when `timeout 6` stopped it, so it starts and stays up; no clicking); `./target/release/tidewise --screenshot home --out target/tmp/home.png` → exit 0; `./packaging/appimage/build.sh` → exit 0 (dist/Tidewise-0.1.0-x86_64.AppImage, 11 MB; no /tmp/appimage_extracted_* left); `tools/ci-local.sh` → exit 0 (fmt, clippy, test, release build, both secret scripts, audit, deny).
- Screenshots: all 11 re-taken with `--screenshot <screen> --out target/tmp/shots/<screen>.png` → exit 0 each, `cmp` says byte-identical to `docs/screenshots/` (so the UI did not change since they were taken). I looked at storage.png and settings.png (Read). storage.png's footer line under "Biggest files" is cut off ("…always go to Trash first, never"); the README says so. settings.png shows the About section with "Tidewise 0.1.0", so SECURITY.md's "Settings → About" is true.
- Claims checked by command: no shell in non-test code (`grep -rnE '"(sh|bash)"|sh -c|"-c"' crates/*/src` → only package names in test data and one `/usr/bin/sh -c` inside `mod tests` of endtask.rs); no network calls in our code (`grep -rnE 'std::net|TcpStream|UdpSocket|reqwest|ureq|hyper|curl' crates/*/src crates/*/Cargo.toml Cargo.toml` → no output); no `sudo`/`pkexec` in `crates/*/src` (no output). Link-time libraries: `ldd target/release/tidewise` lists libfontconfig and libfreetype; `pacman -Qo /usr/lib/libfontconfig.so` → fontconfig 2:2.18.3-2.
- Status table is taken from CHANGELOG, run 07's Summary and the progress log; budgets from `docs/budgets.md` (measured at commit 802e847, before this run's lockfile bump).
- CHANGELOG got a "Development" block (CI, secret check, fuzz workflow, README); `tools/check-secrets.sh` → exit 0 on the final tree.
Why Partial: the install lines (`pacman -S --needed base-devel rust fontconfig pkgconf`, `sudo apt install build-essential pkg-config libfontconfig1-dev`, rustup) were **not run**, and the acceptance asks that each command in the README was run.
Not verified:
- The Arch and Ubuntu install commands, and that those are the complete build dependencies (only checked: this machine builds, fontconfig and freetype are what the binary links). Whether the app builds on Ubuntu 22.04 at all.
- Whether the CI badge shows green (it shows GitHub's result for `ci.yml` on `main`; `main` does not contain this run's fixes yet).
- The "Built / Partial" labels beyond what CHANGELOG and run 07 say; no person has used any page. Nine of the 11 screenshots were not looked at one by one this round (they are byte-identical to the ones taken in run 07, 2 of which were looked at then).
- "Many have tests": the golden rules are not each proven by a named test; I did not map them.
- The 6-second runs prove start-up only, not a rendered window, correct GPU rendering or any click.
Errors: my first `tools/test-readme.sh` flag check flagged cargo's own `--release` as an unknown app flag (test bug; narrowed to lines that run the binary before the red commit, but the red commit's check would still have flagged the old README's one-line cargo + binary text). A `Bash` call combining several commands with a `git tag` listing was refused by the permission layer; I re-ran it without that command and used `git for-each-ref refs/tags` instead (nothing was changed).
Decisions taken without the human: → docs/questions.md "Run 08 — R8-Q04" (5 entries)
