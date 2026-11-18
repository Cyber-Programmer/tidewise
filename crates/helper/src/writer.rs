//! The helper's stdout, written by a thread of its own (review 5.8 R-04, threat model question 34; T-07, T-17).
//!
//! stdout belongs to the caller. A caller that never reads it makes every `write` block, and a block inside the loop that
//! watches pacman is a stopped job (in the middle of `-Sy` that leaves the package lists newer than the packages). So the
//! helper never writes to stdout itself: lines go into a bounded queue and this thread writes them.
//!
//! Two ways in, and the difference is the point:
//! - `offer` is for progress lines. It **never waits**: a line that finds the queue full (too many lines, or too many
//!   bytes) is dropped and counted. The loop that supervises pacman calls only this.
//! - `deliver` is for what must not be lost: a notice, and the final result line. It **waits for room**, but only inside
//!   one budget (`Limits::final_wait`, 5 s) that starts at the first call and is shared by all of them, so a reader that
//!   never reads costs the helper 5 s once, not 5 s per line. It is called only after pacman has finished.
//!
//! `finish` then waits, inside the same budget, until everything queued has been written, so a reader that resumes in time
//! gets every line and the exit code is not returned before the last line left. When the budget is spent the helper goes
//! on and exits (a thread stuck in `write(2)` ends with the process).
//!
//! The memory this can hold: at most `Limits::bytes` queued (1 MiB) plus the one line in the writer's hand. A line is at
//! most `MAX_EVENT_LINE_BYTES` (16 KiB), so 1024 lines alone could have been 16 MiB: hence the byte cap too.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::run::write_line;

/// The bounds of one writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most lines in the queue.
    pub lines: usize,
    /// Most bytes in the queue, each line counted with its new line.
    pub bytes: usize,
    /// How long `deliver` and `finish` may wait in all, from the first of them.
    pub final_wait: Duration,
}

/// A queue in front of one output, and the thread that writes it.
pub struct BoundedWriter {
    shared: Arc<Shared>,
    limits: Limits,
    /// When the shared waiting budget ends. Set by the first `deliver` or `finish`.
    budget_end: OnceLock<Instant>,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    queue: VecDeque<String>,
    /// Bytes in the queue, each line counted with its new line.
    bytes: usize,
    /// The writer has taken a line and has not finished writing it.
    in_hand: bool,
    /// Lines that were not queued (no room, or no writer). A failed write is not counted: a reader that went away
    /// is expected, not interesting.
    dropped: usize,
    /// The writer thread could not be started, or the writer was dropped: nothing more will be written.
    closed: bool,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl BoundedWriter {
    /// Starts the writer thread for `out`.
    pub fn start<W: Write + Send + 'static>(out: W, limits: Limits) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let for_thread = shared.clone();
        let spawned = thread::Builder::new()
            .name("tidewise-output".to_string())
            .spawn(move || write_loop(out, &for_thread));
        if spawned.is_err() {
            // No writer: everything is dropped and counted, and the job still runs.
            lock(&shared.state).closed = true;
        }
        BoundedWriter {
            shared,
            limits,
            budget_end: OnceLock::new(),
        }
    }

    /// Queues `line`, or drops it and counts it. Never waits for the reader: the lock is held for a few instructions and
    /// the writer thread never holds it while writing.
    pub fn offer(&self, line: &str) {
        let mut state = lock(&self.shared.state);
        if state.closed || !self.fits(&state, line) {
            state.dropped = state.dropped.saturating_add(1);
            return;
        }
        push(&mut state, line);
        drop(state);
        self.shared.changed.notify_all();
    }

    /// Queues `line` and, if there is no room, waits for it, but only inside the shared budget. True if the line was
    /// queued. Never used for progress.
    pub fn deliver(&self, line: &str) -> bool {
        let end = self.budget_end();
        let mut state = lock(&self.shared.state);
        loop {
            if state.closed {
                state.dropped = state.dropped.saturating_add(1);
                return false;
            }
            if self.fits(&state, line) {
                push(&mut state, line);
                drop(state);
                self.shared.changed.notify_all();
                return true;
            }
            let Some(left) = time_left(end) else {
                state.dropped = state.dropped.saturating_add(1);
                return false;
            };
            state = wait(&self.shared, state, left);
        }
    }

    /// Waits, inside the shared budget, until everything queued has been written. True if it all was.
    pub fn finish(&self) -> bool {
        let end = self.budget_end();
        let mut state = lock(&self.shared.state);
        loop {
            if state.queue.is_empty() && !state.in_hand {
                return true;
            }
            if state.closed {
                return false;
            }
            let Some(left) = time_left(end) else {
                return false;
            };
            state = wait(&self.shared, state, left);
        }
    }

    /// How many lines were not queued.
    pub fn dropped(&self) -> usize {
        lock(&self.shared.state).dropped
    }

    /// A line fits when the queue is empty (one line always does: the byte cap is above the longest event), or when
    /// both the line cap and the byte cap have room.
    fn fits(&self, state: &State, line: &str) -> bool {
        state.queue.is_empty()
            || (state.queue.len() < self.limits.lines
                && state.bytes.saturating_add(line.len() + 1) <= self.limits.bytes)
    }

    fn budget_end(&self) -> Instant {
        *self
            .budget_end
            .get_or_init(|| Instant::now() + self.limits.final_wait)
    }
}

impl Drop for BoundedWriter {
    fn drop(&mut self) {
        lock(&self.shared.state).closed = true;
        self.shared.changed.notify_all();
    }
}

fn push(state: &mut State, line: &str) {
    state.bytes = state.bytes.saturating_add(line.len() + 1);
    state.queue.push_back(line.to_string());
}

fn time_left(end: Instant) -> Option<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

fn wait<'a>(
    shared: &'a Shared,
    state: MutexGuard<'a, State>,
    left: Duration,
) -> MutexGuard<'a, State> {
    shared
        .changed
        .wait_timeout(state, left)
        .unwrap_or_else(PoisonError::into_inner)
        .0
}

/// The writer thread: takes one line at a time, and writes it with no lock held.
fn write_loop<W: Write>(mut out: W, shared: &Shared) {
    loop {
        let line = {
            let mut state = lock(&shared.state);
            loop {
                if let Some(line) = state.queue.pop_front() {
                    state.bytes = state.bytes.saturating_sub(line.len() + 1);
                    state.in_hand = true;
                    break line;
                }
                if state.closed {
                    return;
                }
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        // Room was made: a `deliver` that was waiting can go on.
        shared.changed.notify_all();
        // The only place the helper writes to stdout. It may block for ever; nothing else waits for it but `finish`,
        // and only inside its budget. A failed write (a reader that went away) is ignored, as it always was.
        let _ = write_line(&mut out, &line);
        lock(&shared.state).in_hand = false;
        shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{FINAL_WRITE_DEADLINE, LINE_QUEUE, QUEUE_BYTES};
    use std::io;
    use std::sync::mpsc;
    use tidewise_helper_protocol::MAX_EVENT_LINE_BYTES;

    fn limits(lines: usize, bytes: usize, final_wait: Duration) -> Limits {
        Limits {
            lines,
            bytes,
            final_wait,
        }
    }

    /// An output that blocks on its first write until the test sends on the sender `new` returns, and then keeps
    /// everything written. It holds a sender of its own, so a test that never releases it has a reader that never reads.
    struct Gate {
        release: mpsc::Receiver<()>,
        _hold: mpsc::Sender<()>,
        open: bool,
        seen: Arc<Mutex<Vec<u8>>>,
    }

    impl Gate {
        fn new() -> (Gate, mpsc::Sender<()>, Arc<Mutex<Vec<u8>>>) {
            let (sender, release) = mpsc::channel();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let gate = Gate {
                release,
                _hold: sender.clone(),
                open: false,
                seen: seen.clone(),
            };
            (gate, sender, seen)
        }
    }

    impl Write for Gate {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if !self.open {
                let _ = self.release.recv();
                self.open = true;
            }
            self.seen.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// An output that fails the first write and keeps the rest.
    struct FailsOnce {
        failed: bool,
        seen: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for FailsOnce {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if !self.failed {
                self.failed = true;
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.seen.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Waits until the writer thread has taken a line and is busy with it.
    fn wait_until_in_hand(writer: &BoundedWriter) {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if lock(&writer.shared.state).in_hand {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("the writer never took a line");
    }

    fn text(seen: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(seen.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn the_queue_holds_no_more_bytes_than_its_cap_though_it_has_room_for_more_lines() {
        let (stuck, _release, _seen) = Gate::new();
        let writer =
            BoundedWriter::start(stuck, limits(LINE_QUEUE, QUEUE_BYTES, FINAL_WRITE_DEADLINE));
        // The biggest line an event can be: 16 KiB with its new line. 1024 of them would be 16 MiB.
        let line = "x".repeat(MAX_EVENT_LINE_BYTES - 1);
        for _ in 0..200 {
            writer.offer(&line);
        }
        let accepted = 200 - writer.dropped();
        // The cap is 1 MiB: 64 such lines in the queue, plus one the writer may already have in its hand.
        let most = QUEUE_BYTES / MAX_EVENT_LINE_BYTES + 1;
        assert!(
            accepted <= most,
            "{accepted} lines of 16 KiB were accepted, about {} KiB queued: the cap is {} KiB",
            accepted * MAX_EVENT_LINE_BYTES / 1024,
            QUEUE_BYTES / 1024
        );
        assert!(
            accepted >= most - 1,
            "a full queue holds its cap, not less: {accepted}"
        );
    }

    // The tests below were written with the code (not red first): the run.rs tests of ff21e96 are the red ones for
    // this behaviour end to end; these pin the parts of the writer on their own.

    #[test]
    fn offer_never_waits_for_a_reader_that_does_not_read() {
        let (stuck, _release, _seen) = Gate::new();
        let writer = BoundedWriter::start(stuck, limits(4, 1024, Duration::from_secs(5)));
        let started = Instant::now();
        for _ in 0..100_000 {
            writer.offer("progress");
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "100 000 offers took {:?}",
            started.elapsed()
        );
        // 4 queued, 1 in the writer's hand (or 4 if it had not started yet); everything else is dropped and counted.
        assert!(writer.dropped() >= 100_000 - 5, "{}", writer.dropped());
    }

    #[test]
    fn the_line_cap_drops_and_counts() {
        let (stuck, _release, _seen) = Gate::new();
        let writer = BoundedWriter::start(stuck, limits(3, 1024 * 1024, Duration::from_secs(5)));
        for _ in 0..10 {
            writer.offer("x");
        }
        let accepted = 10 - writer.dropped();
        assert!((3..=4).contains(&accepted), "{accepted}");
    }

    #[test]
    fn lines_come_out_whole_and_in_order_and_finish_waits_for_them() {
        let (gate, release, seen) = Gate::new();
        release.send(()).unwrap();
        let writer = BoundedWriter::start(gate, limits(16, 1024, Duration::from_secs(5)));
        for line in ["a", "bb", "ccc"] {
            writer.offer(line);
        }
        assert!(writer.deliver("last"));
        assert!(writer.finish());
        assert_eq!(text(&seen), "a\nbb\nccc\nlast\n");
        assert_eq!(writer.dropped(), 0);
    }

    #[test]
    fn deliver_waits_for_room_and_succeeds_when_the_reader_resumes_in_time() {
        let (gate, release, seen) = Gate::new();
        let writer = BoundedWriter::start(gate, limits(2, 1024, Duration::from_secs(10)));
        for _ in 0..10 {
            writer.offer("progress");
        }
        // The queue is full and the reader is stuck: the reader resumes after a moment.
        let resume = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            release.send(()).unwrap();
        });
        assert!(writer.deliver("the final line"), "never dropped");
        assert!(writer.finish());
        resume.join().unwrap();
        assert!(text(&seen).ends_with("the final line\n"), "{}", text(&seen));
    }

    #[test]
    fn deliver_gives_up_when_the_reader_never_resumes_and_the_budget_is_shared() {
        let (stuck, _release, _seen) = Gate::new();
        let writer = BoundedWriter::start(stuck, limits(2, 1024, Duration::from_millis(300)));
        // Let the writer take its line and get stuck on it first: a writer that starts late would take one off a full
        // queue and make room, which is a different situation.
        writer.offer("first");
        wait_until_in_hand(&writer);
        for _ in 0..10 {
            writer.offer("progress");
        }
        let started = Instant::now();
        assert!(!writer.deliver("notice"), "no room, and the budget ends");
        let first = started.elapsed();
        assert!(first >= Duration::from_millis(250), "{first:?}");
        // The budget is shared: a second delivery and the finish have nothing left to wait with.
        assert!(!writer.deliver("final"));
        assert!(!writer.finish());
        assert!(
            started.elapsed() < Duration::from_millis(700),
            "three waits cost {:?}, the budget is 300 ms in all",
            started.elapsed()
        );
    }

    #[test]
    fn a_failed_write_loses_that_line_and_changes_nothing_else() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let out = FailsOnce {
            failed: false,
            seen: seen.clone(),
        };
        let writer = BoundedWriter::start(out, limits(16, 1024, Duration::from_secs(5)));
        writer.offer("lost");
        assert!(writer.finish());
        writer.offer("kept");
        assert!(writer.deliver("final"));
        assert!(writer.finish());
        assert_eq!(text(&seen), "kept\nfinal\n");
        assert_eq!(writer.dropped(), 0, "a failed write is not counted");
    }

    // ---- proofs added by review 6.10a (A-02 and a survivor of its mutation list). They pin behaviour that already
    // ---- existed, so they started green; each was mutation-checked (see the commit message).

    /// An output whose every write takes a while: a line that is "in the writer's hand" for a measurable time.
    struct Slow {
        pause: Duration,
        seen: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for Slow {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            thread::sleep(self.pause);
            self.seen.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn finish_waits_for_a_line_that_is_still_being_written() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let out = Slow {
            pause: Duration::from_millis(300),
            seen: seen.clone(),
        };
        let writer = BoundedWriter::start(out, limits(16, 1024, Duration::from_secs(5)));
        assert!(writer.deliver("only line"));
        // By now the writer has taken the line (the queue is empty) and is in the middle of writing it.
        thread::sleep(Duration::from_millis(50));
        assert!(writer.finish());
        assert_eq!(
            text(&seen),
            "only line\n",
            "finish returned while the line was still being written: the exit would cut it short"
        );
    }

    #[test]
    fn a_line_longer_than_the_cap_is_still_accepted_into_an_empty_queue() {
        // The byte cap must never refuse the only line there is: a final line has to be able to go out.
        let (stuck, _release, _seen) = Gate::new();
        let writer = BoundedWriter::start(stuck, limits(8, 100, Duration::from_secs(5)));
        writer.offer(&"x".repeat(500));
        assert_eq!(writer.dropped(), 0);
    }

    /// Waits until the writer has written everything queued and holds no line.
    fn wait_until_empty(writer: &BoundedWriter) {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            {
                let state = lock(&writer.shared.state);
                if state.queue.is_empty() && !state.in_hand {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("the writer never emptied its queue");
    }

    #[test]
    fn a_lost_notice_means_no_final_line_and_both_are_counted() {
        // Review 6.10a A-04, decided by the human on 18 Nov 2026. The app must never get a result that hides a lost
        // notice: once one `deliver` has failed, every later one fails too, so the final line is not written and the
        // app sees "no result". The outcome stays in the exit code and the journal.
        let (stuck, release, seen) = Gate::new();
        let writer = BoundedWriter::start(stuck, limits(2, 1024, Duration::from_millis(300)));
        writer.offer("first");
        wait_until_in_hand(&writer);
        for _ in 0..5 {
            writer.offer("progress");
        }
        let dropped_before = writer.dropped();
        assert!(!writer.deliver("notice"), "no room, and the budget ends");

        // Room appears just after the budget is spent: the reader resumes and the queue drains.
        release.send(()).unwrap();
        wait_until_empty(&writer);
        assert!(
            !writer.deliver("final"),
            "a final line written after a lost notice would hide the loss from the app"
        );
        assert_eq!(
            writer.dropped(),
            dropped_before + 2,
            "the lost notice and the refused final line are both counted"
        );
        let written = text(&seen);
        assert!(
            !written.contains("notice") && !written.contains("final"),
            "{written}"
        );
    }
}
