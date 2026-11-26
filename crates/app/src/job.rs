//! One helper run as the screens follow it (WBS 6.4b, shared with 6.5): the ticket that starts it, what it has said so
//! far, and why it did not end well. No Slint, no threads.

use crate::helper_request::{HelperRequest, RequestError};
use crate::helper_run::JobEnd;
use tidewise_helper_protocol::{Event, Notice, Phase as Stage, Progress};

/// One helper run to start: the number says which job events belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTicket {
    pub number: u64,
    pub request: HelperRequest,
}

/// The running helper job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub number: u64,
    /// `None` until the helper says anything: until then the password window may still be open.
    pub progress: Option<Progress>,
    /// 0 to 100, never going backwards.
    pub percent: u8,
    pub notices: Vec<Notice>,
}

impl Job {
    pub fn new(number: u64) -> Self {
        Self {
            number,
            progress: None,
            percent: 0,
            notices: Vec::new(),
        }
    }

    /// A `Progress` or `Notice` event. The result comes through the end of the run, not here.
    pub fn event(&mut self, event: Event) {
        match event {
            Event::Progress(progress) => {
                self.percent = self.percent.max(percent_of(progress));
                self.progress = Some(progress);
            }
            Event::Notice(notice) => self.notices.push(notice),
            Event::Finished(_) => {}
        }
    }
}

/// Why a job did not end well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFailure {
    /// No request was made: a bug.
    NotSent(RequestError),
    /// The helper run ended without success. The notices it sent first are kept: they may say what did change.
    Ended { end: JobEnd, notices: Vec<Notice> },
}

/// How far along the whole job is, from one phase's count: checking 0–5, downloading 5–50, installing 50–92, hooks
/// from 92. The helper's phases have no weights of their own; these are the board's.
fn percent_of(progress: Progress) -> u8 {
    let (from, to) = match progress.phase {
        Stage::Checking => (0, 5),
        Stage::Downloading => (5, 50),
        Stage::Installing => (50, 92),
        Stage::Hooks => (92, 92),
    };
    let total = u64::from(progress.total.max(1));
    let done = u64::from(progress.current).min(total);
    let within = (to - from) * done / total;
    u8::try_from(from + within).unwrap_or(100)
}
