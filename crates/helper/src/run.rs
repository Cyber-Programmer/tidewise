//! The whole start, in order (threats T-01, T-02, T-07, T-08, T-14, T-15).
//!
//! `run` takes everything it needs as arguments: the facts about the process (`Facts`, gathered by `main`), stdin,
//! stdout, stderr and a log function. Nothing here touches the real system, so every branch is a unit test.
//!
//! Order of the start checks, each one refusing before the next is looked at:
//! 1. only the expected file descriptors are open (`fds`)
//! 2. the effective user id is 0 (`uid`)
//! 3. the working directory is `/` (done by `main`; the result is passed in, and moving to `/` is harmless even when
//!    a later check refuses)
//! 4. argv is exactly the program name and one summary (`args`)
//!
//! Only then is stdin read (`input`, capped), the request decided (`gate`), one journal entry written (`journal`)
//! and one result line printed. A refusal to start prints one fixed sentence on **stderr**, nothing on stdout, writes
//! no journal entry, and never reads stdin.
//!
//! Exit codes:
//! - `0` the job finished (not used in 5.3a: there are no jobs)
//! - `1` a result line with an error was printed on stdout (the request was refused or is not implemented yet)
//! - `2` refused to start (see `StartRefusal`); `pkexec` itself uses 126 and 127 (`pkexec(1)`)
//!
//! Output never goes through `println!` and friends: they panic when stdout or stderr is closed or full. `write_line`
//! returns the error, and `run` ignores it on purpose: the exit code and the journal entry must not depend on whether
//! anyone is still listening.

use std::ffi::OsString;
use std::io::{self, Read, Write};

use crate::fds::FdEntry;

/// The job finished. Unused until 5.3b.
pub const EXIT_OK: u8 = 0;
/// A result line with an error was printed.
pub const EXIT_ERROR_LINE: u8 = 1;
/// The helper refused to start.
pub const EXIT_REFUSED_TO_START: u8 = 2;

/// What `main` found out about the process before calling `run`.
#[derive(Debug)]
pub struct Facts {
    /// `/proc/self/fd`, as read by `fds::read_fd_entries`.
    pub fds: io::Result<Vec<FdEntry>>,
    /// Our own process id (to recognise our own directory listing among the descriptors).
    pub own_pid: u32,
    /// The text of `/proc/self/status`.
    pub status: io::Result<String>,
    /// The result of `chdir("/")`.
    pub chdir: io::Result<()>,
    /// argv, program name first.
    pub args: Vec<OsString>,
    /// The `PKEXEC_UID` variable, the only one read.
    pub pkexec_uid: Option<OsString>,
}

/// Why the helper did not start. Each has one fixed sentence; nothing from the caller is ever repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRefusal {
    CannotCheckFds,
    UnexpectedFd,
    CannotCheckUser,
    NotRoot,
    CannotChangeDirectory,
    BadArguments,
}

impl StartRefusal {
    /// The sentence printed on stderr.
    pub fn text(self) -> &'static str {
        match self {
            Self::CannotCheckFds => {
                "tidewise-helper: could not check the open files, so it did not start. Nothing was changed."
            }
            Self::UnexpectedFd => {
                "tidewise-helper: it was started with extra open files, so it did not start. Nothing was changed."
            }
            Self::CannotCheckUser => {
                "tidewise-helper: could not check which user it runs as, so it did not start. Nothing was changed."
            }
            Self::NotRoot => {
                "tidewise-helper: it must be started as root through pkexec, so it did not start. Nothing was changed."
            }
            Self::CannotChangeDirectory => {
                "tidewise-helper: could not move to the root folder, so it did not start. Nothing was changed."
            }
            Self::BadArguments => {
                "tidewise-helper: it must be started with exactly one argument, the summary, so it did not start. Nothing was changed."
            }
        }
    }
}

/// Writes `line` and a new line in one call, then flushes. The error is returned, never a panic.
pub fn write_line(out: &mut impl Write, line: &str) -> io::Result<()> {
    let _ = (out, line);
    Ok(())
}

/// Runs the whole start and returns the exit code.
pub fn run(
    facts: Facts,
    stdin: impl Read,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    log: &mut dyn FnMut(&[u8]),
) -> u8 {
    let _ = (facts, stdin, stdout, stderr, log);
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tidewise_helper_protocol::{ErrorKind, Event, MAX_REQUEST_BYTES, Outcome, parse_event};

    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const UPDATE_SUMMARY: &str = "update system (3 packages)";

    const NOT_IMPLEMENTED_LINE: &str = "{\"finished\":{\"error\":\"not_implemented\"}}\n";

    fn std_fds() -> Vec<FdEntry> {
        vec![
            (0, "/dev/null".to_string()),
            (1, "pipe:[1]".to_string()),
            (2, "pipe:[2]".to_string()),
            (3, "/proc/77/fd".to_string()),
        ]
    }

    fn good_facts(summary: &str) -> Facts {
        Facts {
            fds: Ok(std_fds()),
            own_pid: 77,
            status: Ok("Name:\thelper\nUid:\t0\t0\t0\t0\n".to_string()),
            chdir: Ok(()),
            args: vec![
                OsString::from("/usr/lib/tidewise/tidewise-helper"),
                OsString::from(summary),
            ],
            pkexec_uid: Some(OsString::from("1000")),
        }
    }

    struct Outcome1 {
        code: u8,
        stdout: String,
        stderr: String,
        logged: Vec<Vec<u8>>,
        stdin_touched: bool,
    }

    /// stdin that remembers whether anyone read from it.
    struct Watched<'a> {
        inner: Cursor<Vec<u8>>,
        touched: &'a std::cell::Cell<bool>,
    }

    impl Read for Watched<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.touched.set(true);
            self.inner.read(buf)
        }
    }

    fn go(facts: Facts, stdin: &[u8]) -> Outcome1 {
        let touched = std::cell::Cell::new(false);
        let reader = Watched {
            inner: Cursor::new(stdin.to_vec()),
            touched: &touched,
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut logged: Vec<Vec<u8>> = Vec::new();
        let code = run(facts, reader, &mut stdout, &mut stderr, &mut |datagram| {
            logged.push(datagram.to_vec())
        });
        Outcome1 {
            code,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
            logged,
            stdin_touched: touched.get(),
        }
    }

    fn only_log(outcome: &Outcome1) -> String {
        assert_eq!(outcome.logged.len(), 1, "exactly one journal entry");
        String::from_utf8(outcome.logged[0].clone()).unwrap()
    }

    // ---- a request that gets through the start ----------------------------------------------------------

    #[test]
    fn a_valid_request_prints_one_not_implemented_line_and_logs_once() {
        let outcome = go(good_facts(UPDATE_SUMMARY), UPDATE.as_bytes());
        assert_eq!(outcome.code, EXIT_ERROR_LINE);
        assert_eq!(outcome.stdout, NOT_IMPLEMENTED_LINE);
        assert_eq!(outcome.stderr, "");
        let log = only_log(&outcome);
        assert!(log.contains("TIDEWISE_CALLER_UID=1000\n"), "{log}");
        assert!(log.contains("TIDEWISE_OUTCOME=not_implemented\n"), "{log}");
        assert!(
            log.contains("TIDEWISE_SUMMARY=update system (3 packages)\n"),
            "{log}"
        );
    }

    #[test]
    fn stdout_holds_exactly_one_event_line_and_nothing_else() {
        let outcome = go(good_facts(UPDATE_SUMMARY), UPDATE.as_bytes());
        let lines: Vec<&str> = outcome.stdout.split_inclusive('\n').collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with('\n'));
        assert_eq!(
            parse_event(lines[0].trim_end()),
            Ok(Event::Finished(Outcome::Error(ErrorKind::NotImplemented)))
        );
    }

    #[test]
    fn a_wrong_summary_is_a_mismatch_line_and_is_logged() {
        let outcome = go(good_facts("update system (9 packages)"), UPDATE.as_bytes());
        assert_eq!(outcome.code, EXIT_ERROR_LINE);
        assert_eq!(
            outcome.stdout,
            "{\"finished\":{\"error\":\"summary_mismatch\"}}\n"
        );
        let log = only_log(&outcome);
        assert!(log.contains("TIDEWISE_OUTCOME=summary_mismatch\n"), "{log}");
        assert!(
            !log.contains("9 packages"),
            "argv text must not be logged: {log}"
        );
    }

    #[test]
    fn a_malformed_request_is_a_malformed_line_without_a_summary_in_the_log() {
        let outcome = go(good_facts(UPDATE_SUMMARY), b"{");
        assert_eq!(outcome.code, EXIT_ERROR_LINE);
        assert_eq!(outcome.stdout, "{\"finished\":{\"error\":\"malformed\"}}\n");
        let log = only_log(&outcome);
        assert!(log.contains("TIDEWISE_OUTCOME=malformed\n"), "{log}");
        assert!(!log.contains("TIDEWISE_SUMMARY"), "{log}");
    }

    #[test]
    fn an_empty_stdin_is_a_malformed_request() {
        let outcome = go(good_facts(UPDATE_SUMMARY), b"");
        assert_eq!(outcome.stdout, "{\"finished\":{\"error\":\"malformed\"}}\n");
    }

    #[test]
    fn a_request_over_the_cap_is_too_large() {
        let big = vec![b' '; MAX_REQUEST_BYTES + 5000];
        let outcome = go(good_facts(UPDATE_SUMMARY), &big);
        assert_eq!(outcome.stdout, "{\"finished\":{\"error\":\"too_large\"}}\n");
    }

    #[test]
    fn a_hostile_pkexec_uid_is_logged_as_unknown() {
        let mut facts = good_facts(UPDATE_SUMMARY);
        facts.pkexec_uid = Some(OsString::from("0\nMESSAGE=pwned"));
        let outcome = go(facts, UPDATE.as_bytes());
        let log = only_log(&outcome);
        assert!(log.contains("TIDEWISE_CALLER_UID=unknown\n"), "{log}");
        assert!(!log.contains("pwned"), "{log}");
        assert_eq!(outcome.stdout, NOT_IMPLEMENTED_LINE);
    }

    #[test]
    fn the_callers_identity_never_changes_the_answer() {
        for uid in [None, Some("0"), Some("1000"), Some("x"), Some("4294967295")] {
            let mut facts = good_facts(UPDATE_SUMMARY);
            facts.pkexec_uid = uid.map(OsString::from);
            assert_eq!(
                go(facts, UPDATE.as_bytes()).stdout,
                NOT_IMPLEMENTED_LINE,
                "{uid:?}"
            );
        }
    }

    // ---- refusing to start ------------------------------------------------------------------------------

    fn refused(facts: Facts, expected: StartRefusal) {
        let outcome = go(facts, UPDATE.as_bytes());
        assert_eq!(outcome.code, EXIT_REFUSED_TO_START, "{expected:?}");
        assert_eq!(outcome.stdout, "", "{expected:?}");
        assert_eq!(
            outcome.stderr,
            format!("{}\n", expected.text()),
            "{expected:?}"
        );
        assert!(outcome.logged.is_empty(), "{expected:?}: no journal entry");
        assert!(
            !outcome.stdin_touched,
            "{expected:?}: stdin must not be read"
        );
    }

    #[test]
    fn every_start_check_refuses_with_its_own_sentence_before_stdin_is_read() {
        let mut unreadable_fds = good_facts(UPDATE_SUMMARY);
        unreadable_fds.fds = Err(io::Error::other("x"));
        refused(unreadable_fds, StartRefusal::CannotCheckFds);

        let mut extra_fd = good_facts(UPDATE_SUMMARY);
        extra_fd.fds = Ok({
            let mut fds = std_fds();
            fds.push((5, "/etc/shadow".to_string()));
            fds
        });
        refused(extra_fd, StartRefusal::UnexpectedFd);

        let mut unreadable_status = good_facts(UPDATE_SUMMARY);
        unreadable_status.status = Err(io::Error::other("x"));
        refused(unreadable_status, StartRefusal::CannotCheckUser);

        let mut garbage_status = good_facts(UPDATE_SUMMARY);
        garbage_status.status = Ok("nothing useful".to_string());
        refused(garbage_status, StartRefusal::NotRoot);

        let mut user = good_facts(UPDATE_SUMMARY);
        user.status = Ok("Uid:\t1000\t1000\t1000\t1000\n".to_string());
        refused(user, StartRefusal::NotRoot);

        let mut no_chdir = good_facts(UPDATE_SUMMARY);
        no_chdir.chdir = Err(io::Error::other("x"));
        refused(no_chdir, StartRefusal::CannotChangeDirectory);

        for args in [
            vec![],
            vec![OsString::from("helper")],
            vec![
                OsString::from("helper"),
                OsString::from("a"),
                OsString::from("b"),
            ],
        ] {
            let mut bad_args = good_facts(UPDATE_SUMMARY);
            bad_args.args = args;
            refused(bad_args, StartRefusal::BadArguments);
        }
    }

    #[test]
    fn the_checks_run_in_the_documented_order() {
        // Everything wrong at once: the first check in the list is the one that speaks.
        let mut all_wrong = good_facts(UPDATE_SUMMARY);
        all_wrong.fds = Ok(vec![
            (0, "x".to_string()),
            (1, "x".to_string()),
            (2, "x".to_string()),
            (9, "x".to_string()),
        ]);
        all_wrong.status = Ok("Uid:\t1\t1\t1\t1\n".to_string());
        all_wrong.chdir = Err(io::Error::other("x"));
        all_wrong.args = vec![];
        refused(all_wrong, StartRefusal::UnexpectedFd);

        let mut not_root_and_bad_args = good_facts(UPDATE_SUMMARY);
        not_root_and_bad_args.status = Ok("Uid:\t1\t1\t1\t1\n".to_string());
        not_root_and_bad_args.args = vec![];
        refused(not_root_and_bad_args, StartRefusal::NotRoot);

        let mut chdir_and_bad_args = good_facts(UPDATE_SUMMARY);
        chdir_and_bad_args.chdir = Err(io::Error::other("x"));
        chdir_and_bad_args.args = vec![];
        refused(chdir_and_bad_args, StartRefusal::CannotChangeDirectory);
    }

    #[test]
    fn a_refusal_never_repeats_what_the_caller_sent() {
        let mut facts = good_facts("SECRET-ARGV");
        facts.status = Ok("Uid:\t1000\t1000\t1000\t1000\n".to_string());
        facts.pkexec_uid = Some(OsString::from("SECRET-UID"));
        let outcome = go(facts, b"SECRET-STDIN");
        assert!(!outcome.stderr.contains("SECRET"));
        assert_eq!(outcome.stdout, "");
    }

    #[test]
    fn the_refusal_sentences_are_fixed_plain_and_different() {
        let all = [
            StartRefusal::CannotCheckFds,
            StartRefusal::UnexpectedFd,
            StartRefusal::CannotCheckUser,
            StartRefusal::NotRoot,
            StartRefusal::CannotChangeDirectory,
            StartRefusal::BadArguments,
        ];
        for (i, a) in all.iter().enumerate() {
            let text = a.text();
            assert!(text.starts_with("tidewise-helper: "), "{text}");
            assert!(text.ends_with("Nothing was changed."), "{text}");
            assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)), "{text}");
            assert!(!text.contains('/'), "no paths in {text}");
            for b in all.iter().skip(i + 1) {
                assert_ne!(a.text(), b.text());
            }
        }
    }

    #[test]
    fn the_exit_codes_are_the_documented_ones() {
        assert_eq!(EXIT_OK, 0);
        assert_eq!(EXIT_ERROR_LINE, 1);
        assert_eq!(EXIT_REFUSED_TO_START, 2);
    }

    // ---- no print macros: a closed or full stdout or stderr must not panic -----------------------------

    /// A writer that always fails, like a closed pipe (`BrokenPipe`) or `/dev/full` (`StorageFull`).
    struct Failing(io::ErrorKind);

    impl Write for Failing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(self.0))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(self.0))
        }
    }

    /// A writer that accepts everything and fails only when flushed (a buffered stdout on `/dev/full`).
    struct FailsOnFlush;

    impl Write for FailsOnFlush {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::StorageFull))
        }
    }

    #[test]
    fn write_line_writes_the_line_and_a_new_line() {
        let mut out = Vec::new();
        write_line(&mut out, "hello").unwrap();
        assert_eq!(out, b"hello\n");
    }

    #[test]
    fn write_line_returns_the_error_instead_of_panicking() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::StorageFull,
            io::ErrorKind::PermissionDenied,
        ] {
            assert!(write_line(&mut Failing(kind), "x").is_err());
        }
        assert!(write_line(&mut FailsOnFlush, "x").is_err());
    }

    fn run_with_stdout(stdout: &mut impl Write) -> (u8, Vec<Vec<u8>>) {
        let mut logged = Vec::new();
        let code = run(
            good_facts(UPDATE_SUMMARY),
            Cursor::new(UPDATE.as_bytes().to_vec()),
            stdout,
            &mut Vec::new(),
            &mut |datagram| logged.push(datagram.to_vec()),
        );
        (code, logged)
    }

    #[test]
    fn a_stdout_that_cannot_be_written_still_gives_exit_1_and_the_journal_entry() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::StorageFull,
            io::ErrorKind::WriteZero,
        ] {
            let (code, logged) = run_with_stdout(&mut Failing(kind));
            assert_eq!(code, EXIT_ERROR_LINE, "{kind:?}");
            assert_eq!(logged.len(), 1, "{kind:?}");
        }
        let (code, logged) = run_with_stdout(&mut FailsOnFlush);
        assert_eq!(code, EXIT_ERROR_LINE);
        assert_eq!(logged.len(), 1);
    }

    #[test]
    fn a_stderr_that_cannot_be_written_still_gives_exit_2_and_no_journal_entry() {
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::StorageFull] {
            let mut facts = good_facts(UPDATE_SUMMARY);
            facts.status = Ok("Uid:\t1000\t1000\t1000\t1000\n".to_string());
            let mut logged = Vec::new();
            let code = run(
                facts,
                Cursor::new(Vec::new()),
                &mut Vec::new(),
                &mut Failing(kind),
                &mut |datagram| logged.push(datagram.to_vec()),
            );
            assert_eq!(code, EXIT_REFUSED_TO_START, "{kind:?}");
            assert!(logged.is_empty());
        }
    }

    #[test]
    fn the_journal_entry_is_written_before_the_result_line_is_printed() {
        // If printing hangs or the app goes away, the audit trail already exists.
        struct Recorder<'a>(&'a std::cell::RefCell<Vec<&'static str>>);
        impl Write for Recorder<'_> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.borrow_mut().push("stdout");
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let events = std::cell::RefCell::new(Vec::new());
        let mut stdout = Recorder(&events);
        run(
            good_facts(UPDATE_SUMMARY),
            Cursor::new(UPDATE.as_bytes().to_vec()),
            &mut stdout,
            &mut Vec::new(),
            &mut |_| events.borrow_mut().push("journal"),
        );
        assert_eq!(events.borrow().first(), Some(&"journal"));
        assert!(events.borrow().contains(&"stdout"));
    }
}
