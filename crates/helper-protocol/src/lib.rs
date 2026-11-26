//! The language the Tidewise app and the root helper speak: requests, validation, the pkexec summary, events.
//!
//! No root, no pacman, no I/O: everything here is a pure function on values, so every check can be tested.
//! The helper trusts nothing about its caller (threat model A1): any process of the user can send it a request,
//! so every request goes through `parse_request`, which refuses first and asks questions later.
//! Library code must not panic, so clippy forbids `unwrap`, `expect`, indexing and `panic!` here
//! (tests may still use them).
//!
//! # Talking to the helper (for the app that starts it)
//!
//! The helper's stdin and stdout belong to its caller, and the helper does not rely on the caller being well behaved
//! (threat model A1, question 34). Three rules for the app's side:
//!
//! 1. **Write the request from a thread, then close stdin.** A request can be bigger than the 64 KiB a pipe holds, and
//!    the helper may be writing to the app at the same moment, so write the request from a thread of its own (in one
//!    go or in pieces), and drop the pipe when the request is complete: that is how the helper knows it has all of it.
//!    The helper waits at most **10 seconds for the whole request**. A stdin that is still silent after that is refused
//!    as `Malformed` and nothing runs. A request that arrives in pieces inside the 10 seconds is fine.
//! 2. **Read stdout to its end, line by line, and treat `Progress` as best effort.** Progress lines can be missing when
//!    the app falls behind (they are dropped, never queued without limit), so never count them or wait for one.
//!    `Notice` and `Finished` wait for room, but only inside **one budget of 5 seconds in all**, and the helper must see
//!    everything queued written inside it: an app that never reads, or reads too slowly (a busy UI thread), stops the
//!    helper from waiting after that, and the job is not affected by it. So read on a thread of its own, never on the
//!    one that draws the window.
//! 3. **`Finished` is the only result.** If it never arrives, or only part of it does (a last line with no new line
//!    at its end is not a result), or the helper lost a `Notice` and so did not write it at all, the helper's exit code (0 ok, 1 the job ended with an error, 2 refused to start) and
//!    the journal entry still say how the job ended; do not guess from silence.
//!
//! The numbers (10 s, 5 s, a queue of 1024 lines and 1 MiB) are in `tidewise-helper`, `run.rs`, and pinned by a test
//! there; the reasons are in `docs/security/threat-model.md`, question 34.
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

mod event;
mod parse;
mod request;
mod summary;
mod validate;

pub use event::{
    Details, ErrorKind, Event, EventError, MAX_DETAILS_BYTES, MAX_EVENT_LINE_BYTES, Notice,
    Outcome, Phase, Progress, event_line, parse_event,
};
pub use parse::{MAX_REQUEST_BYTES, ProtocolError, parse_request, request_json};
pub use request::{
    Action, Install, PROTOCOL_VERSION, PackageRef, ProviderChoice, Request, TransactionItem,
    UpdateSystem,
};
pub use summary::{MAX_SUMMARY_BYTES, summary};
pub use validate::{
    Field, InvalidRequest, MAX_NAME_BYTES, MAX_PROVIDERS, MAX_TARGETS, MAX_TRANSACTION,
    MAX_VERSION_BYTES, Reason, is_valid_transaction_item, validate,
};
