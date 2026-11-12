//! The testable part of the Tidewise app: view models that turn pacman data into the values a screen shows.
//!
//! No Slint types live here, so everything can be unit-tested. Library code must not panic, so clippy
//! forbids `unwrap`, `expect`, indexing and `panic!` here (tests may still use them).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

pub mod apps_model;
pub mod apps_view;
pub mod list_scroll;
