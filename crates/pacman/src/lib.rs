//! Reads pacman data (installed packages, search, details, dry-runs). No UI, no root.
//!
//! The parsers are pure functions on `&str`: they never run pacman and never touch the file system.
//! The `run` module is the only place that starts a process (read-only, no root, no shell).
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
mod qu;
mod record;
mod run;
mod si;
mod size;
mod ss;

pub use error::{PacmanError, ParseError};
pub use qi::{InstallReason, InstalledPackage, parse_qi};
pub use qq::parse_qq;
pub use qu::{PendingUpdate, parse_qu};
pub use run::{installed_packages, package_details, pending_updates, search};
pub use si::{
    Comparison, Dependency, DetailsLookup, OptionalDependency, PackageDetails, PackageRef,
    VersionRule, parse_si,
};
pub use ss::{SearchResult, parse_ss};
