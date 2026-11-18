//! The helper's stdout, written by a thread of its own (review 5.8 R-04, threat model question 34; T-07, T-17).
//!
//! stdout belongs to the caller. A caller that never reads it makes every `write` block, and a block inside the loop that
//! watches pacman is a stopped job (in the middle of `-Sy` that leaves the package lists newer than the packages). So the
//! helper never writes to stdout itself: lines go into a bounded queue and this thread writes them. A line that finds the
//! queue full is dropped and counted; the supervision loop never waits for the reader.
//!
//! COMPILE SHIM (6.10a, byte-cap red commit): the queue is bounded by lines only. The byte cap, the waiting delivery of
//! notices and of the final line, and the drain are added in the green commit.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

use crate::run::write_line;

/// A queue in front of one output, and the thread that writes it.
pub struct BoundedWriter {
    shared: Arc<Shared>,
    max_lines: usize,
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
    /// Lines that were not delivered: the queue was full, or the write failed.
    dropped: usize,
    closed: bool,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl BoundedWriter {
    /// Starts the writer thread for `out`.
    pub fn start<W: Write + Send + 'static>(out: W, max_lines: usize, _max_bytes: usize) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let for_thread = shared.clone();
        let spawned = thread::Builder::new()
            .name("tidewise-output".to_string())
            .spawn(move || write_loop(out, &for_thread));
        if spawned.is_err() {
            lock(&shared.state).closed = true;
        }
        BoundedWriter { shared, max_lines }
    }

    /// Queues `line`, or drops it and counts it. Never waits for the reader.
    pub fn offer(&self, line: &str) {
        let mut state = lock(&self.shared.state);
        if state.closed || state.queue.len() >= self.max_lines {
            state.dropped = state.dropped.saturating_add(1);
            return;
        }
        state.bytes = state.bytes.saturating_add(line.len() + 1);
        state.queue.push_back(line.to_string());
        drop(state);
        self.shared.changed.notify_all();
    }

    /// How many lines were not delivered.
    pub fn dropped(&self) -> usize {
        lock(&self.shared.state).dropped
    }
}

impl Drop for BoundedWriter {
    fn drop(&mut self) {
        lock(&self.shared.state).closed = true;
        self.shared.changed.notify_all();
    }
}

fn write_loop<W: Write>(mut out: W, shared: &Shared) {
    loop {
        let line = {
            let mut state = lock(&shared.state);
            loop {
                if let Some(line) = state.queue.pop_front() {
                    state.bytes = state.bytes.saturating_sub(line.len() + 1);
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
        let written = write_line(&mut out, &line).is_ok();
        if !written {
            let mut state = lock(&shared.state);
            state.dropped = state.dropped.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{LINE_QUEUE, QUEUE_BYTES};
    use std::io;
    use std::sync::mpsc;
    use tidewise_helper_protocol::MAX_EVENT_LINE_BYTES;

    /// An output whose first write never finishes: a reader that never reads.
    struct Stuck {
        wait: mpsc::Receiver<()>,
        _hold: mpsc::Sender<()>,
    }

    impl Stuck {
        fn new() -> Stuck {
            let (hold, wait) = mpsc::channel();
            Stuck { wait, _hold: hold }
        }
    }

    impl Write for Stuck {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            let _ = self.wait.recv();
            Ok(0)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_queue_holds_no_more_bytes_than_its_cap_though_it_has_room_for_more_lines() {
        let writer = BoundedWriter::start(Stuck::new(), LINE_QUEUE, QUEUE_BYTES);
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
}
