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
mod qi;
mod qq;
mod run;
mod size;

pub use error::{PacmanError, ParseError};
pub use qi::{InstallReason, InstalledPackage, parse_qi};
pub use qq::parse_qq;
pub use run::installed_packages;
