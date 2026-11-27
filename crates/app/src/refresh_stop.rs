//! Telling a running list refresh to stop (WBS 6.5): on Cancel, when another check replaces it, and on the way out of
//! the app, which otherwise leaves its pacman running with nobody to end it. No Slint.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// The worker's side: give `cancel()` to the refresh, and drop this when the refresh has returned.
pub struct RefreshFlag {
    flag: Arc<AtomicBool>,
    _running: Sender<()>,
}

impl RefreshFlag {
    pub fn cancel(&self) -> &AtomicBool {
        &self.flag
    }
}

/// The screen's side: the one refresh it may have running.
#[derive(Default)]
pub struct Refreshes {
    current: Option<(Arc<AtomicBool>, Receiver<()>)>,
}

impl Refreshes {
    /// A new refresh. The one before it, if it is still running, is told to stop.
    pub fn start(&mut self) -> RefreshFlag {
        let flag = Arc::new(AtomicBool::new(false));
        let (running, finished) = mpsc::channel();
        self.current = Some((flag.clone(), finished));
        RefreshFlag {
            flag,
            _running: running,
        }
    }

    /// Tells the current refresh to stop. Does not wait for it.
    pub fn stop(&mut self) {}

    /// Stops it and waits until it has returned. `false`: it had not after `limit`.
    pub fn stop_and_wait(&mut self, limit: Duration) -> bool {
        let _ = limit;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    fn stopped(flag: &RefreshFlag) -> bool {
        flag.cancel().load(Ordering::Relaxed)
    }

    #[test]
    fn stop_sets_the_running_refreshs_flag() {
        let mut refreshes = Refreshes::default();
        let flag = refreshes.start();
        assert!(!stopped(&flag));
        refreshes.stop();
        assert!(stopped(&flag));
    }

    #[test]
    fn a_new_refresh_stops_the_one_before_and_not_itself() {
        let mut refreshes = Refreshes::default();
        let first = refreshes.start();
        let second = refreshes.start();
        assert!(stopped(&first));
        assert!(!stopped(&second));
    }

    #[test]
    fn leaving_waits_until_the_refresh_has_returned() {
        let mut refreshes = Refreshes::default();
        let flag = refreshes.start();
        let worker = std::thread::spawn(move || {
            while !flag.cancel().load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(100));
        });
        let started = Instant::now();
        assert!(refreshes.stop_and_wait(Duration::from_secs(5)));
        assert!(started.elapsed() >= Duration::from_millis(100));
        worker.join().unwrap();
    }

    #[test]
    fn leaving_does_not_wait_for_ever_and_has_nothing_to_wait_for_without_a_refresh() {
        let mut refreshes = Refreshes::default();
        assert!(refreshes.stop_and_wait(Duration::from_secs(5)));
        let _stuck = refreshes.start();
        assert!(!refreshes.stop_and_wait(Duration::from_millis(50)));
    }
}
