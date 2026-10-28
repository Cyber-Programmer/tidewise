# Engineering practices

How Tidewise is built, tested and secured. `CLAUDE.md` holds the short rules; this file holds the
detail and the reasons. Each practice names where it comes from, so it can be checked, not just trusted.

## 1. Test-driven development

**Cycle, every behaviour change:** red → green → refactor.

1. Write the test. Run it. **Show it failing** for the right reason (an assertion about the missing
   behaviour, not a compile error).
2. Write the smallest code that makes it pass. Run it. Green.
3. Clean up with the tests still green.

**Make it auditable with two commits** per behaviour change:

```
test(core): process list skips threads (red)      ← test only; CI may fail on this commit
feat(core): skip threads in process list          ← code that makes it pass
```

If a test truly can't be committed failing on its own (for example, a compile-time API), put the failing
test output in `docs/progress.md` under the task instead, and say why.

**Shape of the suite (test pyramid).** Google's guidance is a pyramid of roughly 70 % unit, 20 %
integration and 10 % end-to-end tests: fast, focused tests at the bottom, few slow ones at the top
([Google Testing Blog](https://testing.googleblog.com/2015/04/just-say-no-to-more-end-to-end-tests.html)).

| Layer | What | Where | Speed |
| --- | --- | --- | --- |
| Unit | Parsers from fixtures, maths, view models, helper validation, grouping | `#[cfg(test)]` next to code | ms |
| Property | Invariants over many generated inputs (sizes never negative, rows never exceed RAM) | `proptest` (ask before adding) | ms–s |
| Integration | Real commands in distro containers; helper end to end as root in a container | `tests/`, `tests/distro/` | s–min |
| Visual | Design check against `docs/design/png/` | `tools/design_diff.py` | s |
| End to end | AppImage starts on Ubuntu 22.04, newest Ubuntu, Arch; first-run flow | VM checklist + scripted launch | min |
| Performance budget | Idle memory and CPU stay under budget (section 5) | `tools/measure.sh` | min |

Rules: tests are deterministic (no clock, network or random without a fixed seed); a flaky test is a bug;
never delete or weaken a failing test to get green; fix the code or ask.

## 2. Security engineering

Modelled on Microsoft's Security Development Lifecycle: threat modeling, static analysis, fuzz testing,
review by someone other than the author, and penetration testing
([Microsoft SDL](https://learn.microsoft.com/en-us/windows/security/threat-protection/msft-security-dev-lifecycle)).

**Threat model** — `docs/security/threat-model.md`, updated in every milestone that adds a privileged
action, a file write, a parser of outside data or a network call. For each: what can go wrong (STRIDE:
spoofing, tampering, repudiation, information disclosure, denial of service, elevation of privilege),
the defence, and the test that proves the defence.

**Static analysis, every CI run:** `cargo clippy -D warnings`, `cargo audit` (known vulnerabilities),
`cargo deny` (licences, advisories, duplicate crates), a secret scanner on the repo (ask before adding the
tool). `unsafe` is forbidden in `crates/helper` and `crates/helper-protocol` (`#![forbid(unsafe_code)]`).

**Fuzz testing** — `cargo fuzz` targets for everything that parses untrusted input: the helper's JSON
request, the pacman/apt/dnf/flatpak/snap parsers, `/proc` parsers, `.desktop` parsing. Each target runs at
least 10 minutes before a milestone closes; crashes become regression tests. `cargo fuzz` needs a nightly
toolchain — ask the human to install it if missing ([cargo-fuzz](https://docs.rs/cargo-fuzz)).

**Security tests (our "pen test" in code)** — for the helper and anything running as root:
- hostile-input table: `--force`, `--`, `../`, `*`, absolute paths, NUL bytes, unicode look-alikes,
  empty, 10 000 names, deeply nested JSON, oversize request;
- privilege tests in a container: the helper refuses when not started through polkit, refuses unknown
  actions, never runs a shell, clears the environment, and leaves files outside its allow-list untouched;
- TOCTOU: the confirmed list is re-checked right before acting;
- symlink attacks: a cache path replaced by a symlink to `/etc` is refused.

**Adversarial review** — before any change to `crates/helper*` or `packaging/polkit` is called done, a
fresh subagent that did not write the code reviews the diff against the threat model, trying to break it,
and reports findings in `docs/security/reviews/`. The human still reviews every line.

**Human penetration test** — a real pen test is people attacking the finished app. Before v1.0: the
human runs `docs/security/pentest-checklist.md` on a VM, and the project asks for an outside review of the
helper and polkit policy. Automated tests above make that review cheaper; they don't replace it.

**Supply chain** — follow OpenSSF Scorecard checks where they apply: branch protection, CI tests, code
review, pinned dependencies (GitHub Actions pinned to commit SHAs), dependency update tool, fuzzing,
SAST, `SECURITY.md`, signed releases, least-privilege workflow tokens
([OpenSSF Scorecard](https://github.com/ossf/scorecard)).

## 3. Small, reviewable changes

- One task = one focused change. Prefer several small commits over one large one; a reviewer should
  understand each commit in a few minutes (the same idea as Google's "small CLs").
- Commit messages follow Conventional Commits: `feat:`, `fix:`, `test:`, `refactor:`, `docs:`, `perf:`,
  `chore:`, with a scope, e.g. `fix(core): …`. Breaking changes say `BREAKING CHANGE:` in the body.
- Each commit builds and passes tests, except a `test(…): … (red)` commit.
- Never mix refactoring with behaviour changes in one commit.

## 4. Decisions, docs and history

- Architecture decisions get a short record in `docs/decisions/NNNN-title.md`: context, options, decision,
  consequences.
- Public behaviour changes go in `CHANGELOG.md` (Keep a Changelog style), versions follow SemVer.
- When something goes wrong (a bad release, a data-loss bug), write a blameless postmortem in
  `docs/postmortems/`: what happened, impact, root cause, what changes so it can't recur.

## 5. Budgets (they are tests)

| Budget | Limit | Checked by |
| --- | --- | --- |
| Idle memory, software renderer | ≤ 40 MB PSS | `tools/measure.sh` |
| Idle CPU, Home page | < 2 % of one core | `tools/measure.sh` |
| Design match | LAYOUT MISMATCH ≤ 10 % per reference image | `tools/design_diff.py` |
| AppImage size | to be set in M2 | build script |

A change that breaks a budget is a bug, even if every unit test passes.

## 6. Accessibility and plain language

Real buttons and labels, full keyboard use, 4.5:1 text contrast, reduced motion honoured, plain-language
text first with technical detail behind the switch. Checked on every new screen.
