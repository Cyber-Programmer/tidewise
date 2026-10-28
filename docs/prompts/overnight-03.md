# Overnight run 3 — correct numbers, low CPU, sidebar, then start M2 (unattended)

You are working alone overnight. Nobody can answer questions until morning.
Read `CLAUDE.md`, `docs/design/DESIGN-CHECK.md` and the M1 entries in `docs/questions.md` first.
CLAUDE.md rules apply, with these overrides **for this run only**:

- **No waiting for approval.** Write a 3–5 line plan per task into `docs/progress.md`, then do it.
- Several tasks per session is fine; finish one fully (tests green, committed) before the next.
- **Pre-approved dependencies:** none new. Everything needed is already in `Cargo.lock`. Anything
  else → `docs/questions.md`.
- When stuck on a decision only the human can make: write the question + your recommended answer to
  `docs/questions.md`, take the safest reversible option, keep going.

## Hard limits (never)

- No `sudo`, `pkexec`, `su`, nothing needing root. Missing system package → `docs/questions.md`, skip.
- No work on `crates/helper`, `crates/helper-protocol`, `packaging/polkit`.
- No files changed outside this repository (cargo caches excepted).
- No `git push`, merge, rebase, force or history rewrites. Local commits only.
- No destructive system commands, and nothing that deletes files outside a temp dir the test made.
- Don't edit anything in `docs/design/`.

## Git

Branch `overnight/2026-10-29` from `main`. One commit per finished task: `R3-Tn: <what changed>`.

## Tasks, in order (test-driven: failing test first, every time)

**R3-T1 — Bug: memory above 16 GB and CPU above 100 % per process (top priority).**
Human saw single rows showing more memory than the machine has, and CPU over 100 %.
Root causes found in review (confirm each with a failing test first):
1. *Threads counted as processes.* On Linux, `sysinfo` lists threads (tasks) in `System::processes()`;
   each thread reports its parent's whole resident memory. `crates/core/src/stats.rs` maps every entry,
   and grouping then sums them, so a 400 MB app with 100 threads shows ~40 GB. The spike screenshots
   show thread names as rows (`HeapHelper`, `JITWorker`, `mi-scavenger`, `CPU 1/KVM`).
   Fix: skip entries where `Process::thread_kind()` is `Some(_)`. Test with a fake `/proc` tree that has
   a process with `task/` threads, or through the sampling seam if one exists.
2. *CPU unit.* `cpu_usage()` is "percent of one core" and can reach 100 × cores (1200 % on this
   12-thread machine). Users from Windows expect "% of the whole PC". Show per-process and per-group CPU
   as share of the whole machine (divide by the logical CPU count), 0–100. Keep the per-core value in
   technical details as "x % of one core". Update the field doc comment.
3. *Shared memory counted many times in a group.* Summing resident memory (RSS) across Firefox's 14
   processes counts shared libraries 14 times. For each process read `/proc/<pid>/statm` and use
   private memory = (resident − shared) × page size for the group sum. Show RSS of a single process in
   technical details. Fixture tests for `statm` parsing and the group sum.
Acceptance: an invariant test that, on this real machine, the sum of all rows' memory is at most total
RAM, and no row's CPU exceeds 100 %. Demo mode and design-check scores unchanged (re-run the check).

**R3-T2 — Idle CPU under 2 % of one core.** Baseline first with `tools/measure.sh` on Home, Performance
and Processes, both renderers; write the numbers down. Then:
- refresh the process table only while Processes (or the Performance top-3) is visible; use
  `ProcessesToUpdate::All` only there, and a cheap refresh elsewhere;
- cache per-pid display names (cmdline, `.desktop` lookup) and only read new pids;
- push new values into Slint models only when they changed; avoid rebuilding whole models each tick;
- check whether the software renderer repaints the full window each tick; only dirty regions should
  repaint.
Measure after each change, keep what helps, record a before → after table in `docs/progress.md`.
Target: under 2 % on Home with the software renderer. If you can't reach it, record where time goes
(`perf` is fine if installed; don't install anything) and stop at the best result.

**R3-T3 — Sidebar fits a 600 px tall window.** Below 720 px of height, make the nav scroll (or compress
item height to 36 px); logo stays at the top and History/Settings stay reachable. Add a screenshot test at
1360×600 that checks Settings is inside the visible area. Design-check scores at 960 px must not change.

**R3-T4 — Theme names match tokens.md.** `tokens.md` now lists the extra shades under "Extra shades used
by the artboards". Rename the Theme entries to those names and extend the token test to cover them.

**R3-T5 (stretch) — M2 first tasks** from `docs/prompts/m2-storage-test-build.md`: task 1 (disk scanner in
`core`, temp-tree tests first) and, if time remains, task 2 (Storage screen to `png/Storage.png` with the
design-check loop). Read-only: nothing is deleted or moved.

## Done means

- R3-T1 to R3-T4 committed, each with its tests; R3-T5 committed or written up as not reached.
- `docs/progress.md` has the CPU before → after table and a morning summary.
- Final `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all` exit 0.
