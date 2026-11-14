//! The language the Tidewise app and the root helper speak: requests, validation, the pkexec summary, events.
//!
//! No root, no pacman, no I/O: everything here is a pure function on values, so every check can be tested.
//! The helper trusts nothing about its caller (threat model A1): any process of the user can send it a request,
//! so every request goes through `parse_request`, which refuses first and asks questions later.
//! Library code must not panic, so clippy forbids `unwrap`, `expect`, indexing and `panic!` here
//! (tests may still use them).
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

mod parse;
mod request;
mod validate;

pub use parse::{MAX_REQUEST_BYTES, ProtocolError, parse_request};
pub use request::{
    Action, Install, PROTOCOL_VERSION, PackageRef, ProviderChoice, Request, TransactionItem,
    UpdateSystem,
};
pub use validate::{
    Field, InvalidRequest, MAX_NAME_BYTES, MAX_PROVIDERS, MAX_TARGETS, MAX_TRANSACTION,
    MAX_VERSION_BYTES, Reason, validate,
};
