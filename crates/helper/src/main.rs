//! The `tidewise-helper` binary. It only gathers the real inputs and hands them to the library, so everything
//! that matters can be tested without being root. Same code rules as the library (see `lib.rs`).
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::print_stdout,
        clippy::print_stderr,
        clippy::dbg_macro
    )
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // Not wired yet: refuse everything until the gate exists (exit code 2 = refused to start).
    ExitCode::from(2)
}
