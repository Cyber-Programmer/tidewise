//! Glue only (WBS 6.5): shows how long the running helper job has taken, once a second, so a phase without a
//! percentage does not look stuck. The words are `tidewise_app::job::elapsed_text`.

use std::time::{Duration, Instant};

use slint::{Timer, TimerMode};
use tidewise_app::job::elapsed_text;

#[derive(Default)]
pub struct JobClock {
    timer: Timer,
}

impl JobClock {
    /// Starts counting, unless it already does. `show` is called now and then every second.
    pub fn start(&self, show: impl Fn(String) + 'static) {
        if self.timer.running() {
            return;
        }
        let started = Instant::now();
        show(elapsed_text(0));
        self.timer
            .start(TimerMode::Repeated, Duration::from_secs(1), move || {
                show(elapsed_text(started.elapsed().as_secs()));
            });
    }

    pub fn stop(&self) {
        self.timer.stop();
    }
}
