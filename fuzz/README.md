# Fuzzing the pacman parsers and the helper protocol

Six cargo-fuzz (libFuzzer) targets: `qi` (`parse_qi`), `qq` (`parse_qq`), `ss` (`parse_ss`), `si` (`parse_si`), `qu` (`parse_qu`)
and `request` (`tidewise-helper-protocol`: `parse_request`, `validate`, `summary`, `parse_event`, `Details::from_raw`). They only call
parsers and pure functions on bytes or text; they never run pacman or touch the system. Needs **nightly**, used only through `cargo +nightly`
(the default toolchain stays stable). `fuzz/` is its own workspace, so the root fmt/clippy/test/deny and CI do
not see it.

Seeds: `tests/fixtures/pacman/` is used directly as the read-only seed folder (no copies, so they can't drift);
`request` uses `tests/fixtures/helper-protocol/` (three valid requests).
New inputs libFuzzer finds go to `fuzz/corpus/<target>/`; crashes go to `fuzz/artifacts/<target>/`.
Both are gitignored.

Run from the repo root (10 minutes each):

    mkdir -p fuzz/corpus/qi fuzz/corpus/qq fuzz/corpus/ss fuzz/corpus/si fuzz/corpus/qu fuzz/corpus/request
    cargo +nightly fuzz run qi fuzz/corpus/qi tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run qq fuzz/corpus/qq tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run ss fuzz/corpus/ss tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run si fuzz/corpus/si tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run qu fuzz/corpus/qu tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run request fuzz/corpus/request tests/fixtures/helper-protocol -- -max_total_time=600; echo "exit: $?"

No crashes: the output ends with `Done <N> runs in 600 second(s)`, `exit: 0`, and `fuzz/artifacts/` has
nothing in it. A crash prints `panicked at …` or `deadly signal`, then `Test unit written to
fuzz/artifacts/<target>/crash-<hash>`; replay it with `cargo +nightly fuzz run <target> <that file>`.
Turn every crash into a normal unit test (in `crates/pacman` or `crates/helper-protocol`) before fixing it.

The `request` target also checks promises, not only absence of panics: whatever `parse_request` accepts passes `validate`
again; `summary` is printable ASCII and at most 200 bytes for anything serde accepts; an event that parses prints back to a
line that parses to the same event; `Details` has no control characters (except a new line) and at most 4096 bytes.

Check the fuzz code itself (root commands don't cover it):

    cargo fmt --manifest-path fuzz/Cargo.toml --check
    cargo +nightly fuzz build
