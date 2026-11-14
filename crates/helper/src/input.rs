//! Step 6: read the request from stdin with a hard byte cap (threat T-07).
//!
//! The helper never reads more than `MAX_REQUEST_BYTES + 1` bytes, however much the caller sends. The one extra byte
//! is what lets `parse_request` say "too large" (it refuses anything longer than the cap, before parsing); a longer
//! request is never stored.
//!
//! Known limit, recorded for 5.3b: a caller that never closes stdin and sends nothing keeps the helper waiting here.
//! That is harmless in 5.3a, because nothing is held yet (no lock, no sync, no pacman). In 5.3b the whole request
//! must be read and validated before any lock is taken, any sync is done or pacman is started.

use std::io::Read;

use tidewise_helper_protocol::MAX_REQUEST_BYTES;

/// The request could not be read to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    ReadFailed,
}

/// Reads at most `MAX_REQUEST_BYTES + 1` bytes. Interrupted reads are retried by the standard library; any other
/// read error is `ReadFailed`.
pub fn read_capped(reader: impl Read) -> Result<Vec<u8>, InputError> {
    let _ = reader;
    Ok(Vec::new())
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
}
