//! Reads pacman data (installed packages, search, details, dry-runs). No UI, no root.
//!
//! The parsers are pure functions on `&str`: they never run pacman and never touch the file system.
//! Library code must not panic, so clippy forbids `unwrap`, `expect`, indexing and `panic!` here
//! (tests may still use them).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

mod error;
mod name;
mod qq;

pub use error::ParseError;
pub use qq::parse_qq;
