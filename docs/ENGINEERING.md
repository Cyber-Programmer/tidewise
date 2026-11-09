# Engineering practices (v2, Arch-only rebuild)

`CLAUDE.md` holds the short rules; this file holds the detail and the reasons.

## 1. Test-driven development

Every behaviour change: **red → green → refactor**.
1. Write the test, run it, show it failing for the right reason (an assertion, not a compile error).
2. Write the smallest code that passes. 3. Clean up with tests green.

Two commits per change: `test(scope): … (red)` then `feat|fix(scope): …`.

| Layer | What | Where |
|---|---|---|
| Unit (most) | Parsers from fixtures, size maths, view models, helper validation | `#[cfg(test)]` next to code |
| Integration | Real pacman in an Arch container; helper end to end as root in a container | `tests/container/` |
| Visual | Screens checked against the design | design check (set up in M2) |
| Manual | Things a test can't open, e.g. a real window on the desktop | recorded in `docs/progress.md` as a bent rule |
| End to end (few) | Install the PKGBUILD on a clean Arch VM, run the main flows | manual checklist |

Rules: deterministic tests (no clock/network/randomness without a fixed seed); a flaky test is a bug;
never weaken or delete a failing test to get green. **Never run real destructive pacman commands on the host** —
use fixtures or the container.

UI rule: keep logic out of `.slint` files and out of `main.rs`. Put it in `core`/`pacman` or a view model,
where it can be tested; the window then only displays tested values.

## 2. Security

Based on Microsoft's SDL: threat modelling, static analysis, fuzzing, independent review, pen testing.

- **Threat model** `docs/security/threat-model.md` (STRIDE), updated whenever a milestone adds a privileged
  action, a file write, a parser of outside data or a network call. Each threat: defence + the test proving it.
- **Static analysis every CI run:** clippy `-D warnings` and `cargo deny check` (RustSec advisories — the same
  database `cargo audit` uses — plus licences, bans and sources; config in `deny.toml`).
  `#![forbid(unsafe_code)]` in `crates/helper*`.
- **Fuzzing:** `cargo fuzz` targets for the helper's JSON request and every pacman / `/proc` parser.
  ≥ 10 minutes each before a milestone closes; crashes become regression tests (needs nightly).
- **Hostile-input table for the helper:** `--force`, `--`, `../`, `*`, absolute paths, NUL bytes, unicode
  look-alikes, empty, 10 000 names, deeply nested JSON, oversize request, package names starting with `-`.
- **Privilege tests (container):** refuses unknown actions, never runs a shell, clears env, leaves files outside
  its allow-list untouched, re-checks the confirmed list (TOCTOU), refuses symlinked cache paths.
- **pacman-specific:** no partial upgrades (golden rule 4) has a test; lock file never touched; protected
  packages refused in the helper, not only the UI.
- **Adversarial review:** before helper/polkit work is done, a fresh subagent reviews the diff against the threat
  model and writes findings to `docs/security/reviews/`. The human still reviews every line.
- **Before v1.0:** human runs `docs/security/pentest-checklist.md` on a VM; ask for an outside review of helper + polkit.
- **Supply chain:** every new dependency needs the human's OK and a decision record if it's big (e.g. Slint);
  `Cargo.lock` is committed; GitHub Actions pinned to commit SHAs, least-privilege tokens, `SECURITY.md`, signed tags.

## 3. Small, reviewable changes

One task = one focused change; each commit understandable in a few minutes. Conventional Commits with scope.
Each commit builds and passes tests, except a `(red)` test commit. Never mix refactor with behaviour change.
Review every change with `git --no-pager diff` (local) or the pull-request diff (GitHub) before it lands.

## 4. Decisions, docs, history

Decision records in `docs/decisions/` (context, options, decision, consequences). `CHANGELOG.md` (Keep a
Changelog), SemVer. Blameless postmortems in `docs/postmortems/` when something goes wrong.

## 5. Budgets (they are tests)

| Budget | Limit |
|---|---|
| Idle memory (software renderer) | ≤ 40 MB PSS |
| Idle CPU on Home | < 2 % of one core |
| Live stats refresh | ≤ 1 per second, paused when window hidden |

## 6. Accessibility and plain language

Real buttons and labels, full keyboard use, 4.5:1 contrast, reduced motion honoured, plain language first,
technical detail behind the switch. Every screen has empty, loading and error states.
