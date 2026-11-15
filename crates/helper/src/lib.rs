//! `tidewise-helper`: the only part of Tidewise that runs as root (golden rules 1 and 2).
//!
//! It is started by `pkexec` with one argument (a human-readable summary) and one JSON request on stdin. In 5.3a
//! it is only the *gate*: it checks how it was started, reads the request with a hard size cap, validates it,
//! compares the summary with the one it recomputes, writes one journal entry, prints one result line, and stops.
//! There is no pacman call anywhere in this crate yet (the jobs arrive in 5.3b).
//!
//! The helper trusts nothing about its caller (threat model A1). Every step is a small function that takes its
//! input as an argument, so it can be tested without being root.
//!
//! Rules this crate keeps in code, not only in prose:
//! - `#![forbid(unsafe_code)]`.
//! - No `unwrap`, `expect`, indexing or `panic!` (clippy denies them outside tests).
//! - No `println!`, `print!`, `eprintln!`, `eprint!` or `dbg!`: they panic when stdout or stderr is closed or full.
//!   Output goes through one function that handles the write error (clippy denies the macros outside tests).
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

pub mod args;
pub mod env;
pub mod fds;
pub mod gate;
pub mod input;
pub mod journal;
pub mod pacman_args;
pub mod run;
pub mod uid;
