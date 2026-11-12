# Fuzzing the pacman parsers

Two cargo-fuzz (libFuzzer) targets: `qi` (`parse_qi`) and `qq` (`parse_qq`). They only call the parsers on text;
they never run pacman or touch the system. Needs **nightly**, used only through `cargo +nightly`
(the default toolchain stays stable). `fuzz/` is its own workspace, so the root fmt/clippy/test/deny and CI do
not see it.

Seeds: `tests/fixtures/pacman/` is used directly as the read-only seed folder (no copies, so they can't drift).
New inputs libFuzzer finds go to `fuzz/corpus/<target>/`; crashes go to `fuzz/artifacts/<target>/`.
Both are gitignored.

Run from the repo root (10 minutes each):

    mkdir -p fuzz/corpus/qi fuzz/corpus/qq
    cargo +nightly fuzz run qi fuzz/corpus/qi tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"
    cargo +nightly fuzz run qq fuzz/corpus/qq tests/fixtures/pacman -- -max_total_time=600; echo "exit: $?"

No crashes: the output ends with `Done <N> runs in 600 second(s)`, `exit: 0`, and `fuzz/artifacts/` has
nothing in it. A crash prints `panicked at …` or `deadly signal`, then `Test unit written to
fuzz/artifacts/<target>/crash-<hash>`; replay it with `cargo +nightly fuzz run <target> <that file>`.
Turn every crash into a normal unit test in `crates/pacman` before fixing it.

Check the fuzz code itself (root commands don't cover it):

    cargo fmt --manifest-path fuzz/Cargo.toml --check
    cargo +nightly fuzz build
