//! Reads pacman data (installed packages, search, details, dry-runs). No UI, no root.
//!
//! The parsers are pure functions on `&str`: they never run pacman and never touch the file system.
//! The `run` module is the only place that starts a process (no root, no shell). All of it only reads, except
//! `private_lists`, which has pacman refresh the app's own private copy of the package lists.
//! Library code must not panic, so clippy forbids `unwrap`, `expect`, indexing and `panic!` here
//! (tests may still use them).
//!
//! No `unsafe`: the root helper links this crate, and the other crates it links already forbid it (review 5.8).
#![forbid(unsafe_code)]
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
mod partial;
mod picks;
mod print;
mod private_lists;
mod qi;
mod qq;
mod qu;
mod record;
mod resolve;
mod run;
mod sg;
mod si;
mod size;
mod ss;
mod sync;
#[cfg(test)]
mod testing;
mod update;

pub use error::{PacmanError, ParseError};
pub use partial::{InstallDecision, install_decision};
pub use picks::{ProviderPick, UpdateFacts, dependency_text, provider_picks, update_picks};
pub use print::{
    PrintFailure, PrintedPackage, UnsatisfiedDependency, parse_print, parse_print_failure,
};
pub use private_lists::{Lists, NotFresh, RefreshWhy, refresh_private_lists};
pub use qi::{InstallReason, InstalledPackage, parse_qi};
pub use qq::parse_qq;
pub use qu::{PendingUpdate, parse_qu};
pub use resolve::{
    Blocker, Candidate, GroupAnswer, GroupChoice, InstallPlan, MAX_ROUNDS, PlanItem,
    ProviderAnswer, Question, Resolution, ResolveInput, Role,
};
pub use run::{
    UpdateLookup, exact_pattern, installed_packages, package_details, pending_updates,
    pending_updates_in, resolve, search, sync_lists, update_lookup,
};
pub use sg::{GroupMember, parse_sg};
pub use si::{
    Comparison, Dependency, DetailsLookup, OptionalDependency, PackageDetails, PackageRef,
    VersionRule, parse_si,
};
pub use ss::{SearchResult, parse_ss};
pub use sync::SyncLists;
pub use update::{Conflict, HeldBlock, Replacement, UpdatePlan, held_block, update_plan};
