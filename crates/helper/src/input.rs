//! Step 6: read the request from stdin with a hard byte cap (threat T-07).
//!
//! The helper never reads more than `MAX_REQUEST_BYTES + 1` bytes, however much the caller sends. The one extra byte
//! is what lets `parse_request` say "too large" (it refuses anything longer than the cap, before parsing); a longer
//! request is never stored.
//!
//! The caller owns stdin too (review 5.8 R-04, question 34). A caller that never closes it and sends nothing used to keep
//! the helper waiting here for ever, and one that sent the request hours later released an action the user had approved
//! hours earlier. So the read has a deadline for the whole request (`read_within`): the reading happens in a thread
//! that owns stdin and a sender and nothing else, and the helper waits for its answer for a fixed time. After a timeout
//! the thread is left alone. It holds nothing the rest of the helper needs (no lock, no journal, no writer, no gate), and
//! if it ever gets an answer the sender finds the receiver gone and the answer is thrown away.
//!
//! Nothing is held while this waits (no lock, no sync, no pacman): the whole request is read and validated before any of
//! that, so a refusal here changes nothing.

use std::io::Read;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use tidewise_helper_protocol::MAX_REQUEST_BYTES;

/// The request could not be read to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    ReadFailed,
    /// The whole request did not arrive inside the deadline.
    TimedOut,
}

/// Reads at most `MAX_REQUEST_BYTES + 1` bytes. Interrupted reads are retried by the standard library; any other
/// read error is `ReadFailed`.
pub fn read_capped(reader: impl Read) -> Result<Vec<u8>, InputError> {
    let limit = u64::try_from(MAX_REQUEST_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| InputError::ReadFailed)?;
    Ok(bytes)
}

/// `read_capped` with a deadline for the whole request, not for each read: a caller that drips one byte at a time is
/// bounded too. The reader is moved into a thread of its own; see the top of this file for what that thread holds.
pub fn read_within(
    reader: impl Read + Send + 'static,
    deadline: Duration,
) -> Result<Vec<u8>, InputError> {
    let (sender, receiver) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("tidewise-input".to_string())
        .spawn(move || {
            let _ = sender.send(read_capped(reader));
        });
    if spawned.is_err() {
        return Err(InputError::ReadFailed);
    }
    match receiver.recv_timeout(deadline) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(InputError::TimedOut),
        Err(RecvTimeoutError::Disconnected) => Err(InputError::ReadFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    use tidewise_helper_protocol::{ProtocolError, parse_request};

    /// An endless reader that counts how many bytes it handed out.
    struct Endless {
        handed_out: usize,
    }

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            buf.fill(b' ');
            self.handed_out += buf.len();
            Ok(buf.len())
        }
    }

    /// Fails with `kind` once, after `first` bytes, then behaves.
    struct Flaky {
        first: Vec<u8>,
        kind: io::ErrorKind,
        failed: bool,
        rest: Vec<u8>,
    }

    impl Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if !self.first.is_empty() {
                let n = self.first.len().min(buf.len());
                buf[..n].copy_from_slice(&self.first[..n]);
                self.first.drain(..n);
                return Ok(n);
            }
            if !self.failed {
                self.failed = true;
                return Err(io::Error::from(self.kind));
            }
            let n = self.rest.len().min(buf.len());
            buf[..n].copy_from_slice(&self.rest[..n]);
            self.rest.drain(..n);
            Ok(n)
        }
    }

    #[test]
    fn a_small_request_is_read_whole() {
        assert_eq!(
            read_capped(Cursor::new(b"hello".to_vec())),
            Ok(b"hello".to_vec())
        );
    }

    #[test]
    fn nothing_on_stdin_is_an_empty_read_not_an_error() {
        assert_eq!(read_capped(Cursor::new(Vec::new())), Ok(Vec::new()));
    }

    #[test]
    fn a_request_exactly_at_the_cap_is_read_whole() {
        let bytes = vec![b' '; MAX_REQUEST_BYTES];
        assert_eq!(read_capped(Cursor::new(bytes.clone())), Ok(bytes));
    }

    #[test]
    fn one_byte_over_the_cap_is_kept_so_the_parser_can_say_too_large() {
        let bytes = vec![b' '; MAX_REQUEST_BYTES + 1];
        let read = read_capped(Cursor::new(bytes)).unwrap();
        assert_eq!(read.len(), MAX_REQUEST_BYTES + 1);
        assert_eq!(parse_request(&read), Err(ProtocolError::TooLarge));
    }

    #[test]
    fn ten_mebibytes_are_cut_at_the_cap_plus_one() {
        let bytes = vec![b' '; 10 * 1024 * 1024];
        let read = read_capped(Cursor::new(bytes)).unwrap();
        assert_eq!(read.len(), MAX_REQUEST_BYTES + 1);
    }

    #[test]
    fn an_endless_stream_is_never_read_past_the_cap_plus_one() {
        let mut endless = Endless { handed_out: 0 };
        let read = read_capped(&mut endless).unwrap();
        assert_eq!(read.len(), MAX_REQUEST_BYTES + 1);
        assert_eq!(endless.handed_out, MAX_REQUEST_BYTES + 1);
    }

    #[test]
    fn a_read_error_is_read_failed_even_after_some_data() {
        let flaky = Flaky {
            first: b"{\"protocol".to_vec(),
            kind: io::ErrorKind::BrokenPipe,
            failed: false,
            rest: b"_version\":1}".to_vec(),
        };
        assert_eq!(read_capped(flaky), Err(InputError::ReadFailed));
    }

    #[test]
    fn an_error_before_any_data_is_read_failed() {
        let flaky = Flaky {
            first: Vec::new(),
            kind: io::ErrorKind::PermissionDenied,
            failed: false,
            rest: Vec::new(),
        };
        assert_eq!(read_capped(flaky), Err(InputError::ReadFailed));
    }

    #[test]
    fn an_interrupted_read_is_retried() {
        let flaky = Flaky {
            first: b"ab".to_vec(),
            kind: io::ErrorKind::Interrupted,
            failed: false,
            rest: b"cd".to_vec(),
        };
        assert_eq!(read_capped(flaky), Ok(b"abcd".to_vec()));
    }

    #[test]
    fn bytes_are_returned_as_they_are_not_as_text() {
        let raw = vec![0xff, 0x00, b'{', 0xc0];
        assert_eq!(read_capped(Cursor::new(raw.clone())), Ok(raw));
    }

    // ---- the deadline (written with the code; the run.rs tests of ff21e96 are the red ones end to end) ---------

    /// A reader that stays open and says nothing: a read waits for ever.
    struct Silent {
        wait: std::sync::mpsc::Receiver<()>,
        _hold: std::sync::mpsc::Sender<()>,
    }

    impl Read for Silent {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            let _ = self.wait.recv();
            Ok(0)
        }
    }

    #[test]
    fn a_reader_that_stays_silent_times_out() {
        let (hold, wait) = std::sync::mpsc::channel();
        let started = std::time::Instant::now();
        let result = read_within(Silent { wait, _hold: hold }, Duration::from_millis(100));
        assert_eq!(result, Err(InputError::TimedOut));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_request_that_arrives_in_time_is_read_whole() {
        let result = read_within(Cursor::new(b"{}".to_vec()), Duration::from_secs(5));
        assert_eq!(result, Ok(b"{}".to_vec()));
    }

    #[test]
    fn an_empty_stdin_is_an_empty_read_not_a_timeout() {
        let result = read_within(Cursor::new(Vec::new()), Duration::from_secs(5));
        assert_eq!(result, Ok(Vec::new()));
    }

    #[test]
    fn a_read_error_is_still_read_failed() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        assert_eq!(
            read_within(Broken, Duration::from_secs(5)),
            Err(InputError::ReadFailed)
        );
    }
}
