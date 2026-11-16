//! The whole start, in order (threats T-01, T-02, T-07, T-08, T-14, T-15).
//!
//! `run` takes everything it needs as arguments: the facts about the process (`Facts`, gathered by `main`), stdin,
//! stdout, stderr and a log function. Nothing here touches the real system, so every branch is a unit test.
//!
//! Order of the start checks, each one refusing before the next is looked at:
//! 1. the effective user id is 0 (`uid`)
//! 2. only the expected file descriptors are open (`fds`)
//! 3. the working directory is `/` (done by `main`; the result is passed in, and moving to `/` is harmless even when
//!    a later check refuses)
//! 4. argv is exactly the program name and one summary (`args`)
//!
//! Only then is stdin read (`input`, capped) and the request decided (`gate`). A refused request gets one journal
//! entry and one result line. An accepted one gets a journal entry (before any pacman command), a runner (made only
//! now), the job (`flows`) with its progress and notice lines on stdout, a second journal entry with the outcome, and
//! the final line. If either entry could not be written, one notice says so just before the final line: a journal that
//! is down never stops a job (pacman's own log still records every package change). A refusal to start prints one
//! fixed sentence on **stderr**, nothing on stdout, writes no journal entry, and never reads stdin.
//!
//! Exit codes:
//! - `0` the job finished: the final line is `{"finished":"ok"}`
//! - `1` a final line with an error was printed on stdout (the request was refused, or the job stopped)
//! - `2` refused to start (see `StartRefusal`); `pkexec` itself uses 126 and 127 (`pkexec(1)`)
//!
//! Output never goes through `println!` and friends: they panic when stdout or stderr is closed or full. `write_line`
//! returns the error, and `run` ignores it on purpose: the exit code and the journal entry must not depend on whether
//! anyone is still listening.

use std::ffi::OsString;
use std::io::{self, Read, Write};

use tidewise_helper_protocol::{ErrorKind, Event, Notice, Outcome, event_line};

use crate::args::{caller_uid, summary_arg};
use crate::fds::{FdEntry, check_fds};
use crate::flows::{RemovalCheck, run_job};
use crate::gate::{Decision, decide};
use crate::input::read_capped;
use crate::journal::{Notes, Stage, record};
use crate::runner::Runner;
use crate::uid::is_root;

/// The job finished.
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
    WrongProgramPath,
}

impl StartRefusal {
    /// The sentence printed on stderr.
    pub fn text(self) -> &'static str {
        match self {
            Self::CannotCheckFds => {
                "tidewise-helper: could not check the open files, so it did not start. Nothing was changed."
            }
            Self::UnexpectedFd => {
                "tidewise-helper: it was started with unexpected open files, so it did not start. Nothing was changed."
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
            Self::WrongProgramPath => {
                "tidewise-helper: it was not started under its own installed name, so it did not start. Nothing was changed."
            }
        }
    }
}

/// Writes `line` and a new line in one call, then flushes. The error is returned, never a panic.
pub fn write_line(out: &mut impl Write, line: &str) -> io::Result<()> {
    let mut bytes = String::with_capacity(line.len() + 1);
    bytes.push_str(line);
    bytes.push('\n');
    out.write_all(bytes.as_bytes())?;
    out.flush()
}

/// What the start checks hand on: the raw summary argument and the caller's id (for the log only).
struct Started {
    summary: Vec<u8>,
    caller: Option<u32>,
}

/// The four start checks, in order. The first one that fails decides.
fn start(facts: Facts) -> Result<Started, StartRefusal> {
    // Root first: for someone who started the helper by hand, "must be started as root" is the useful message, and
    // a caller's extra descriptors (a CI runner hands out some) must not hide it. Both checks refuse before
    // anything is done, so the order does not change what the helper allows.
    let status = facts.status.map_err(|_| StartRefusal::CannotCheckUser)?;
    if !is_root(&status) {
        return Err(StartRefusal::NotRoot);
    }

    let entries = facts.fds.map_err(|_| StartRefusal::CannotCheckFds)?;
    check_fds(&entries, facts.own_pid).map_err(|_| StartRefusal::UnexpectedFd)?;

    facts
        .chdir
        .map_err(|_| StartRefusal::CannotChangeDirectory)?;

    let summary = summary_arg(facts.args).map_err(|_| StartRefusal::BadArguments)?;
    Ok(Started {
        summary,
        caller: caller_uid(facts.pkexec_uid),
    })
}

/// Makes the runner for a job, only after a request was accepted (so a refused request never touches the work folder).
pub type MakeRunner<'a> = &'a mut dyn FnMut() -> Result<Box<dyn Runner>, ErrorKind>;

/// Runs the whole start and returns the exit code. `log` returns whether the entry was written.
pub fn run(
    facts: Facts,
    stdin: impl Read,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    log: &mut dyn FnMut(&[u8]) -> bool,
    make_runner: MakeRunner,
) -> u8 {
    let started = match start(facts) {
        Ok(started) => started,
        Err(refusal) => {
            // A failed write changes nothing: the exit code is the answer.
            let _ = write_line(stderr, refusal.text());
            return EXIT_REFUSED_TO_START;
        }
    };

    match decide(&started.summary, read_capped(stdin)) {
        Decision::Refused(verdict) => {
            let outcome = Outcome::Error(verdict.error);
            // The audit trail first, so it exists even if nobody is listening to stdout any more.
            let written = log(&record(
                started.caller,
                Stage::Finished(&outcome, Notes::default()),
                verdict.summary.as_deref(),
            ));
            finish(stdout, &outcome, !written)
        }
        Decision::Accepted { request, summary } => {
            // Before any pacman command, so a helper that is killed later still left a trace that a job began.
            let accepted_written = log(&record(started.caller, Stage::Accepted, Some(&summary)));

            let (outcome, notes) = match make_runner() {
                Err(kind) => (Outcome::Error(kind), Notes::default()),
                Ok(mut runner) => {
                    let result = run_job(&request, &mut *runner, &mut |event| {
                        // A failed write changes nothing: the job goes on and the exit code is the answer.
                        let _ = write_line(stdout, &event_line(&event));
                    });
                    // What the notices said goes to the journal too, and a check that did not run is said so there.
                    let notes = Notes {
                        not_installed: result.not_installed,
                        removed: result.removed,
                        removal_check_skipped: result.removal_check == RemovalCheck::Skipped,
                    };
                    (result.outcome, notes)
                }
            };

            let finished_written = log(&record(
                started.caller,
                Stage::Finished(&outcome, notes),
                Some(&summary),
            ));
            finish(stdout, &outcome, !(accepted_written && finished_written))
        }
    }
}

/// The last lines: a notice if an audit entry could not be written (a failed journal never stops a job, the app is
/// told instead), then the final line. Returns the exit code.
fn finish(stdout: &mut impl Write, outcome: &Outcome, audit_failed: bool) -> u8 {
    if audit_failed {
        let _ = write_line(
            stdout,
            &event_line(&Event::Notice(Notice::AuditEntryNotWritten)),
        );
    }
    let _ = write_line(stdout, &event_line(&Event::Finished(outcome.clone())));
    match outcome {
        Outcome::Ok => EXIT_OK,
        Outcome::Error(_) => EXIT_ERROR_LINE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{RunOutput, RunnerError};
    use crate::testing::{Fake, ok, out};
    use std::cell::{Cell, RefCell};
    use std::io::Cursor;
    use std::rc::Rc;
    use tidewise_helper_protocol::{
        Action, Details, MAX_REQUEST_BYTES, TransactionItem, parse_event, parse_request,
    };

    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const UPDATE_SUMMARY: &str = "update system (3 packages)";

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const INSTALL_SUMMARY: &str = "install extra/dbeaver with extra/jre-openjdk (5 packages)";
    const REAL_INSTALL_OUT: &str =
        include_str!("../../../tests/fixtures/helper-jobs/real-install-two.out");

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
        runners_made: u32,
        /// "journal", "runner" and "out" (one per line printed), in the order they happened.
        order: Vec<&'static str>,
    }

    /// stdin that remembers whether anyone read from it.
    struct Watched<'a> {
        inner: Cursor<Vec<u8>>,
        touched: &'a Cell<bool>,
    }

    impl Read for Watched<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.touched.set(true);
            self.inner.read(buf)
        }
    }

    /// stdout that keeps what was written and notes each write in the shared order.
    struct Recorder {
        order: Rc<RefCell<Vec<&'static str>>>,
        bytes: Vec<u8>,
    }

    impl Write for Recorder {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.order.borrow_mut().push("out");
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    type Script = Vec<Result<RunOutput, RunnerError>>;

    /// A runner factory that hands out a fake with this script.
    fn fake(script: Script) -> impl FnMut() -> Result<Box<dyn Runner>, ErrorKind> {
        move || Ok(Box::new(Fake::new(script.clone())) as Box<dyn Runner>)
    }

    /// Everything wired with fakes. Journal entries from number `journal_fails_from` on report "not written".
    fn go_with(
        facts: Facts,
        stdin: &[u8],
        make: &mut dyn FnMut() -> Result<Box<dyn Runner>, ErrorKind>,
        journal_fails_from: usize,
    ) -> Outcome1 {
        let touched = Cell::new(false);
        let reader = Watched {
            inner: Cursor::new(stdin.to_vec()),
            touched: &touched,
        };
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut stdout = Recorder {
            order: order.clone(),
            bytes: Vec::new(),
        };
        let mut stderr = Vec::new();
        let mut logged: Vec<Vec<u8>> = Vec::new();
        let mut runners_made = 0u32;
        let log_order = order.clone();
        let make_order = order.clone();
        let code = {
            let mut log = |datagram: &[u8]| -> bool {
                log_order.borrow_mut().push("journal");
                logged.push(datagram.to_vec());
                logged.len() <= journal_fails_from
            };
            let mut counted = || {
                runners_made += 1;
                make_order.borrow_mut().push("runner");
                make()
            };
            run(
                facts,
                reader,
                &mut stdout,
                &mut stderr,
                &mut log,
                &mut counted,
            )
        };
        let order = order.borrow().clone();
        Outcome1 {
            code,
            stdout: String::from_utf8(stdout.bytes).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
            logged,
            stdin_touched: touched.get(),
            runners_made,
            order,
        }
    }

    /// For requests that must be refused: a runner is available but must never be asked for.
    fn go(facts: Facts, stdin: &[u8]) -> Outcome1 {
        go_with(facts, stdin, &mut fake(Vec::new()), usize::MAX)
    }

    /// For requests that run a job: the fake pacman answers with `script`.
    fn go_job(facts: Facts, stdin: &[u8], script: Script) -> Outcome1 {
        go_with(facts, stdin, &mut fake(script), usize::MAX)
    }

    fn only_log(outcome: &Outcome1) -> String {
        assert_eq!(outcome.logged.len(), 1, "exactly one journal entry");
        String::from_utf8(outcome.logged[0].clone()).unwrap()
    }

    fn log_text(outcome: &Outcome1, index: usize) -> String {
        String::from_utf8(outcome.logged[index].clone()).unwrap()
    }

    fn install_items() -> Vec<TransactionItem> {
        match parse_request(INSTALL.as_bytes()).unwrap().action {
            Action::Install(install) => install.transaction,
            _ => panic!("the fixture is an install"),
        }
    }

    fn print_text(items: &[TransactionItem]) -> String {
        items
            .iter()
            .map(|i| format!("{}/{} {}\n", i.repo, i.name, i.version))
            .collect()
    }

    fn installed_text(items: &[TransactionItem]) -> String {
        std::iter::once("pacman 7.1.0-1\n".to_string())
            .chain(items.iter().map(|i| format!("{} {}\n", i.name, i.version)))
            .collect()
    }

    /// The answers of pacman to a clean install of the fixture: -Qu, dry run, real run, -D, -Q.
    fn clean_install_script() -> Script {
        let items = install_items();
        vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"), // the providers are not installed yet
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]
    }

    fn lines(outcome: &Outcome1) -> Vec<&str> {
        outcome
            .stdout
            .split_inclusive('\n')
            .map(|l| l.trim_end())
            .collect()
    }

    // ---- a request that gets through the start ----------------------------------------------------------

    #[test]
    fn a_valid_install_prints_progress_then_one_ok_line_and_logs_twice() {
        let outcome = go_job(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            clean_install_script(),
        );
        assert_eq!(outcome.code, EXIT_OK);
        assert_eq!(outcome.stderr, "");
        let printed = lines(&outcome);
        assert!(printed.len() >= 3, "{printed:?}");
        assert!(printed[0].starts_with("{\"progress\":"), "{printed:?}");
        assert_eq!(printed.last(), Some(&"{\"finished\":\"ok\"}"));

        assert_eq!(outcome.logged.len(), 2);
        let accepted = log_text(&outcome, 0);
        assert!(accepted.contains("TIDEWISE_STAGE=accepted\n"), "{accepted}");
        assert!(
            accepted.contains("TIDEWISE_CALLER_UID=1000\n"),
            "{accepted}"
        );
        assert!(
            accepted.contains(&format!("TIDEWISE_SUMMARY={INSTALL_SUMMARY}\n")),
            "{accepted}"
        );
        let finished = log_text(&outcome, 1);
        assert!(
            finished.contains("TIDEWISE_STAGE=finished\nTIDEWISE_OUTCOME=ok\n"),
            "{finished}"
        );
        assert!(finished.contains("PRIORITY=6\n"), "{finished}");
    }

    #[test]
    fn every_stdout_line_is_one_event_and_the_last_is_the_only_finished() {
        let outcome = go_job(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            clean_install_script(),
        );
        let events: Vec<Event> = lines(&outcome)
            .iter()
            .map(|line| parse_event(line).unwrap_or_else(|e| panic!("{e:?}: {line}")))
            .collect();
        let finished = events
            .iter()
            .filter(|e| matches!(e, Event::Finished(_)))
            .count();
        assert_eq!(finished, 1);
        assert_eq!(events.last(), Some(&Event::Finished(Outcome::Ok)));
        assert!(!events.iter().any(|e| matches!(e, Event::Notice(_))));
    }

    #[test]
    fn refusals_never_make_a_runner_and_have_one_finished_entry() {
        let too_big = vec![b' '; MAX_REQUEST_BYTES + 5000];
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("update system (9 packages)", UPDATE.as_bytes().to_vec()),
            (UPDATE_SUMMARY, b"{".to_vec()),
            (UPDATE_SUMMARY, Vec::new()),
            (UPDATE_SUMMARY, too_big),
        ];
        for (summary, stdin) in cases {
            let outcome = go(good_facts(summary), &stdin);
            assert_eq!(outcome.runners_made, 0, "{summary}");
            assert_eq!(outcome.code, EXIT_ERROR_LINE);
            let entry = only_log(&outcome);
            assert!(entry.contains("TIDEWISE_STAGE=finished\n"), "{entry}");
            assert_eq!(lines(&outcome).len(), 1);
        }
    }

    #[test]
    fn a_job_that_fails_is_one_error_line_exit_1_and_two_entries() {
        // The dry run lists something else than the confirmed list.
        let script = vec![out(1, "", ""), ok("extra/other 1-1\n")];
        let outcome = go_job(good_facts(INSTALL_SUMMARY), INSTALL.as_bytes(), script);
        assert_eq!(outcome.code, EXIT_ERROR_LINE);
        assert_eq!(
            lines(&outcome),
            vec!["{\"finished\":{\"error\":\"transaction_changed\"}}"]
        );
        assert_eq!(outcome.logged.len(), 2);
        let finished = log_text(&outcome, 1);
        assert!(
            finished.contains("TIDEWISE_OUTCOME=transaction_changed\n"),
            "{finished}"
        );
        assert!(finished.contains("PRIORITY=4\n"), "{finished}");
    }

    #[test]
    fn a_runner_that_cannot_be_made_is_a_finished_error_after_the_accepted_entry() {
        let mut make = || -> Result<Box<dyn Runner>, ErrorKind> {
            Err(ErrorKind::PacmanFailed(Details::from_raw(
                "the helper's work folder could not be used.",
            )))
        };
        let outcome = go_with(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            &mut make,
            usize::MAX,
        );
        assert_eq!(outcome.code, EXIT_ERROR_LINE);
        let printed = lines(&outcome);
        assert_eq!(printed.len(), 1);
        assert!(
            printed[0].starts_with("{\"finished\":{\"error\":{\"pacman_failed\":"),
            "{printed:?}"
        );
        assert_eq!(outcome.runners_made, 1);
        assert_eq!(outcome.logged.len(), 2);
        assert!(log_text(&outcome, 1).contains("TIDEWISE_OUTCOME=pacman_failed\n"));
    }

    #[test]
    fn a_notice_for_missing_packages_comes_right_before_the_final_line() {
        let items = install_items();
        let mut partial = items.clone();
        partial.pop();
        let script = vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&partial)),
        ];
        let outcome = go_job(good_facts(INSTALL_SUMMARY), INSTALL.as_bytes(), script);
        assert_eq!(outcome.code, EXIT_OK);
        let printed = lines(&outcome);
        let n = printed.len();
        assert_eq!(
            printed[n - 2],
            "{\"notice\":{\"packages_not_installed\":1}}"
        );
        assert_eq!(printed[n - 1], "{\"finished\":\"ok\"}");
        // The journal does not call that a clean ok.
        let finished = log_text(&outcome, 1);
        assert!(
            finished.contains("TIDEWISE_OUTCOME=ok_with_notices\nTIDEWISE_NOT_INSTALLED=1\n"),
            "{finished}"
        );
        assert!(finished.contains("PRIORITY=4\n"), "{finished}");
    }

    /// A clean install with the lists you choose before and after the real run (the provider is not in the first list,
    /// so the helper marks it as a dependency and the `-D` answer is used).
    fn lists_script(
        before: Result<RunOutput, RunnerError>,
        after: Result<RunOutput, RunnerError>,
    ) -> Script {
        let items = install_items();
        vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            before,
            ok(REAL_INSTALL_OUT),
            ok(""),
            after,
        ]
    }

    #[test]
    fn a_removal_is_a_notice_and_the_journal_says_ok_with_notices() {
        let items = install_items();
        let script = lists_script(
            ok("gone 1-1\nstays 1-1\n"),
            ok(&format!("stays 1-1\n{}", installed_text(&items))),
        );
        let outcome = go_job(good_facts(INSTALL_SUMMARY), INSTALL.as_bytes(), script);
        assert_eq!(outcome.code, EXIT_OK);
        let printed = lines(&outcome);
        let n = printed.len();
        assert_eq!(printed[n - 2], "{\"notice\":{\"packages_removed\":1}}");
        assert_eq!(printed[n - 1], "{\"finished\":\"ok\"}");
        let finished = log_text(&outcome, 1);
        assert!(
            finished.contains("TIDEWISE_OUTCOME=ok_with_notices\nTIDEWISE_REMOVED=1\n"),
            "{finished}"
        );
        assert!(
            finished.contains("(0 not installed, 1 removed)"),
            "{finished}"
        );
        assert!(finished.contains("PRIORITY=4\n"), "{finished}");
    }

    #[test]
    fn a_removal_check_that_could_not_run_is_logged_as_skipped_and_the_result_is_still_ok() {
        for after in [out(1, "a 1-1\n", "error: x\n"), Err(RunnerError::Spawn)] {
            let script = lists_script(ok(""), after);
            let outcome = go_job(good_facts(INSTALL_SUMMARY), INSTALL.as_bytes(), script);
            assert_eq!(outcome.code, EXIT_OK);
            assert_eq!(lines(&outcome).last(), Some(&"{\"finished\":\"ok\"}"));
            assert!(!outcome.stdout.contains("notice"), "{}", outcome.stdout);
            let finished = log_text(&outcome, 1);
            assert!(
                finished
                    .contains("TIDEWISE_OUTCOME=ok_with_notices\nTIDEWISE_REMOVAL_CHECK=skipped\n"),
                "{finished}"
            );
            assert!(finished.contains("PRIORITY=4\n"), "{finished}");
        }
    }

    #[test]
    fn a_clean_job_logs_a_plain_ok_with_no_notes() {
        let outcome = go_job(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            clean_install_script(),
        );
        let finished = log_text(&outcome, 1);
        for field in [
            "NOT_INSTALLED",
            "REMOVED",
            "REMOVAL_CHECK",
            "ok_with_notices",
        ] {
            assert!(!finished.contains(field), "{finished}");
        }
        assert!(
            finished.contains("TIDEWISE_OUTCOME=ok\n") && finished.contains("PRIORITY=6\n"),
            "{finished}"
        );
    }

    #[test]
    fn a_refusal_has_no_notes() {
        let outcome = go(good_facts("update system (9 packages)"), UPDATE.as_bytes());
        let entry = only_log(&outcome);
        for field in ["NOT_INSTALLED", "REMOVED", "REMOVAL_CHECK"] {
            assert!(!entry.contains(field), "{entry}");
        }
    }

    #[test]
    fn a_journal_that_cannot_be_written_adds_one_notice_and_changes_nothing_else() {
        let notice = "{\"notice\":\"audit_entry_not_written\"}";
        // Every entry fails: the job still runs and ends the same way.
        let mut make = fake(clean_install_script());
        let job = go_with(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            &mut make,
            0,
        );
        assert_eq!(job.code, EXIT_OK);
        assert_eq!(job.runners_made, 1);
        let printed = lines(&job);
        let n = printed.len();
        assert_eq!(
            (printed[n - 2], printed[n - 1]),
            (notice, "{\"finished\":\"ok\"}")
        );
        assert_eq!(
            printed.iter().filter(|l| **l == notice).count(),
            1,
            "one notice, not one per entry"
        );

        // A refusal with a journal that is down says so too.
        let refused = go_with(
            good_facts("update system (9 packages)"),
            UPDATE.as_bytes(),
            &mut fake(vec![]),
            0,
        );
        assert_eq!(refused.code, EXIT_ERROR_LINE);
        assert_eq!(
            lines(&refused),
            vec![notice, "{\"finished\":{\"error\":\"summary_mismatch\"}}"]
        );
    }

    #[test]
    fn only_the_second_entry_failing_also_gives_the_notice() {
        let mut make = fake(clean_install_script());
        let outcome = go_with(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            &mut make,
            1,
        );
        let printed = lines(&outcome);
        let n = printed.len();
        assert_eq!(printed[n - 2], "{\"notice\":\"audit_entry_not_written\"}");
        assert_eq!(outcome.code, EXIT_OK);
    }

    #[test]
    fn the_accepted_entry_comes_before_the_runner_and_the_finished_entry_before_the_final_line() {
        let outcome = go_job(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            clean_install_script(),
        );
        assert_eq!(&outcome.order[..2], &["journal", "runner"]);
        let n = outcome.order.len();
        assert_eq!(&outcome.order[n - 2..], &["journal", "out"]);
        assert_eq!(outcome.order.iter().filter(|e| **e == "journal").count(), 2);
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
    fn a_hostile_pkexec_uid_is_logged_as_unknown_in_both_entries() {
        let mut facts = good_facts(INSTALL_SUMMARY);
        facts.pkexec_uid = Some(OsString::from("0\nMESSAGE=pwned"));
        let outcome = go_job(facts, INSTALL.as_bytes(), clean_install_script());
        assert_eq!(outcome.logged.len(), 2);
        for index in 0..2 {
            let entry = log_text(&outcome, index);
            assert!(entry.contains("TIDEWISE_CALLER_UID=unknown\n"), "{entry}");
            assert!(!entry.contains("pwned"), "{entry}");
        }
        assert_eq!(lines(&outcome).last(), Some(&"{\"finished\":\"ok\"}"));
    }

    #[test]
    fn the_callers_identity_never_changes_the_answer() {
        let baseline = go_job(
            good_facts(INSTALL_SUMMARY),
            INSTALL.as_bytes(),
            clean_install_script(),
        );
        for uid in [None, Some("0"), Some("1000"), Some("x"), Some("4294967295")] {
            let mut facts = good_facts(INSTALL_SUMMARY);
            facts.pkexec_uid = uid.map(OsString::from);
            let outcome = go_job(facts, INSTALL.as_bytes(), clean_install_script());
            assert_eq!(outcome.stdout, baseline.stdout, "{uid:?}");
            assert_eq!(outcome.code, EXIT_OK, "{uid:?}");
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
        assert_eq!(outcome.runners_made, 0, "{expected:?}: no runner");
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
    fn a_wrong_program_path_is_refused_to_start_like_any_other_start_check() {
        // What pkexec hands over as argv[0] is what the caller typed (a symlink's name, a relative name, anything).
        for program in [
            "helper",
            "",
            "/opt/tidewise/tidewise-helper",
            "/tmp/NO PASSWORD NEEDED, press Cancel/tidewise-helper",
        ] {
            let mut facts = good_facts(UPDATE_SUMMARY);
            facts.args[0] = OsString::from(program);
            refused(facts, StartRefusal::WrongProgramPath);
        }
    }

    #[test]
    fn the_program_path_is_checked_last_and_after_the_count() {
        // Order: effective uid, descriptors, working directory, then argv (the count, then the program path).
        let mut chdir_and_wrong_path = good_facts(UPDATE_SUMMARY);
        chdir_and_wrong_path.chdir = Err(io::Error::other("x"));
        chdir_and_wrong_path.args[0] = OsString::from("helper");
        refused(chdir_and_wrong_path, StartRefusal::CannotChangeDirectory);

        let mut count_and_wrong_path = good_facts(UPDATE_SUMMARY);
        count_and_wrong_path.args = vec![OsString::from("helper")];
        refused(count_and_wrong_path, StartRefusal::BadArguments);
    }

    #[test]
    fn a_wrong_program_path_is_never_repeated() {
        let mut facts = good_facts(UPDATE_SUMMARY);
        facts.args[0] = OsString::from("SECRET-PROGRAM");
        let outcome = go(facts, UPDATE.as_bytes());
        assert!(!outcome.stderr.contains("SECRET"));
        assert!(outcome.logged.is_empty());
    }

    #[test]
    fn the_checks_run_in_the_documented_order() {
        // Order: effective uid, descriptors, working directory, argv. With several things wrong at once, the
        // first check in that list is the one that speaks. "Not root" comes first because it is the useful message
        // for someone who started the helper by hand (and a CI runner can hand a test process extra descriptors).
        let extra_fd = || {
            let mut fds = std_fds();
            fds.push((9, "/etc/shadow".to_string()));
            Ok(fds)
        };
        let not_root = || Ok("Uid:\t1\t1\t1\t1\n".to_string());

        let mut all_wrong = good_facts(UPDATE_SUMMARY);
        all_wrong.fds = extra_fd();
        all_wrong.status = not_root();
        all_wrong.chdir = Err(io::Error::other("x"));
        all_wrong.args = vec![];
        refused(all_wrong, StartRefusal::NotRoot);

        let mut extra_fd_not_root = good_facts(UPDATE_SUMMARY);
        extra_fd_not_root.fds = extra_fd();
        extra_fd_not_root.status = not_root();
        refused(extra_fd_not_root, StartRefusal::NotRoot);

        let mut unreadable_fds_not_root = good_facts(UPDATE_SUMMARY);
        unreadable_fds_not_root.fds = Err(io::Error::other("x"));
        unreadable_fds_not_root.status = not_root();
        refused(unreadable_fds_not_root, StartRefusal::NotRoot);

        let mut not_root_bad_args = good_facts(UPDATE_SUMMARY);
        not_root_bad_args.status = not_root();
        not_root_bad_args.args = vec![];
        refused(not_root_bad_args, StartRefusal::NotRoot);

        let mut unreadable_status_extra_fd = good_facts(UPDATE_SUMMARY);
        unreadable_status_extra_fd.status = Err(io::Error::other("x"));
        unreadable_status_extra_fd.fds = extra_fd();
        refused(unreadable_status_extra_fd, StartRefusal::CannotCheckUser);

        // As root: descriptors, then the working directory, then argv.
        let mut root_all_wrong = good_facts(UPDATE_SUMMARY);
        root_all_wrong.fds = extra_fd();
        root_all_wrong.chdir = Err(io::Error::other("x"));
        root_all_wrong.args = vec![];
        refused(root_all_wrong, StartRefusal::UnexpectedFd);

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
            StartRefusal::WrongProgramPath,
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

    /// A whole clean install with this stdout; the journal and the runner are fakes.
    fn job_with_stdout(stdout: &mut impl Write) -> (u8, Vec<Vec<u8>>) {
        let mut logged = Vec::new();
        let mut make = fake(clean_install_script());
        let code = run(
            good_facts(INSTALL_SUMMARY),
            Cursor::new(INSTALL.as_bytes().to_vec()),
            stdout,
            &mut Vec::new(),
            &mut |datagram| {
                logged.push(datagram.to_vec());
                true
            },
            &mut make,
        );
        (code, logged)
    }

    #[test]
    fn a_stdout_that_cannot_be_written_changes_neither_the_job_nor_the_exit_code_nor_the_journal() {
        for kind in [
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::StorageFull,
            io::ErrorKind::WriteZero,
        ] {
            let (code, logged) = job_with_stdout(&mut Failing(kind));
            assert_eq!(code, EXIT_OK, "{kind:?}");
            assert_eq!(logged.len(), 2, "{kind:?}");
        }
        let (code, logged) = job_with_stdout(&mut FailsOnFlush);
        assert_eq!(code, EXIT_OK);
        assert_eq!(logged.len(), 2);
    }

    #[test]
    fn a_stderr_that_cannot_be_written_still_gives_exit_2_and_no_journal_entry() {
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::StorageFull] {
            let mut facts = good_facts(UPDATE_SUMMARY);
            facts.status = Ok("Uid:\t1000\t1000\t1000\t1000\n".to_string());
            let mut logged = Vec::new();
            let mut make = fake(vec![]);
            let code = run(
                facts,
                Cursor::new(Vec::new()),
                &mut Vec::new(),
                &mut Failing(kind),
                &mut |datagram| {
                    logged.push(datagram.to_vec());
                    true
                },
                &mut make,
            );
            assert_eq!(code, EXIT_REFUSED_TO_START, "{kind:?}");
            assert!(logged.is_empty());
        }
    }
}
