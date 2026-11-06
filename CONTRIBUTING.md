# Contributing to Tidewise

Thank you for looking. Tidewise is early software (see the status table in the [README](README.md)), and
small, well-tested changes are the easiest to review. By contributing you agree your work is licensed
under GPL-3.0-or-later, like the rest of the project.

## Build

You need a stable Rust toolchain and the fontconfig development files; the exact lines for Arch and
Ubuntu are in the README under "Install".

```
cargo build --release -p tidewise-app
./target/release/tidewise --demo
```

`--demo` shows the design's sample data, so you can try the app without touching real files.

## Check before you push

One script runs the same commands as the CI pipeline, in the same order, and stops at the first failure:

```
tools/ci-local.sh
```

That is `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo test --all --locked`, a release build, a text-based secret check, `cargo audit` and
`cargo deny check`. All of them must exit 0. `cargo audit` and `cargo deny` need `cargo install
cargo-audit cargo-deny --locked` once.

Never run destructive commands on your own machine to test a change. Tests use temp folders, saved
command output in `tests/fixtures/`, or processes the test itself started.

## Tests first

This project is test-driven. For every behaviour change:

1. Write the test. Run it and see it fail for the right reason (an assertion about the missing
   behaviour, not a compile error). Commit it on its own as `test(scope): what it checks (red)`.
2. Write the smallest code that makes it pass, and commit it as `feat(scope): …` or `fix(scope): …`.
3. Clean up with the tests still green. Do not mix a refactor with a behaviour change in one commit.

A test that fails is fixed by fixing the code, never by deleting, skipping or weakening the test. New
parsers of other programs' output need fixtures from real saved output (including a non-English and a
broken case). The longer version, with the reasons, is in [docs/ENGINEERING.md](docs/ENGINEERING.md).

## Design rules

The screens in `docs/design/` are the source of truth. Use only the colours and spacing in
[docs/design/tokens.md](docs/design/tokens.md): no new colours, shadows, blur or gradients. Fonts are
Geist and Geist Mono from `assets/fonts/`. Every number is shown in the mono font, and every destructive
button names what it removes ("Uninstall GIMP"). Plain-language text comes first; technical details sit
behind the "Show technical details" switch. Check a screen against its reference picture with the loop in
`docs/design/DESIGN-CHECK.md`.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/) with a scope, as in the project's log:

- `feat(core): …` a new behaviour
- `fix(app): …` a bug fix
- `test(scope): … (red)` a failing test, committed alone
- `docs: …` documentation only
- `ci: …` the pipeline, workflows and `tools/`
- `refactor`, `perf` and `chore` follow the same shape

Types in use in the log today: `feat`, `fix`, `test`, `docs`, `ci`.

Keep commits small enough to read in a few minutes.

## Talk to us first

Open an issue before you start on any of these, because they change what the project promises:

- anything that runs as root: `crates/helper`, `crates/helper-protocol` and `packaging/polkit` (the
  helper's allow-list is deliberately small);
- a new dependency, a new network call, or a new place the app writes files;
- a new screen or feature that is not in the design (`docs/PLAN.md` lists what v1.0 contains).

To report a security problem, do not open an issue: follow [SECURITY.md](SECURITY.md).
