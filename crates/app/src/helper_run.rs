//! Running the root helper (WBS 6.4b): `pkexec`, the request on its stdin, its events from stdout, and what the end
//! of it means. This is the app's only program start (golden rules 1 and 3; threat model T-02, T-15).
//!
//! The three rules for the helper's caller (`tidewise-helper-protocol`, module docs) are kept here: the request is
//! written from its own thread and stdin is then closed; stdout is read to its end on a worker thread, never on the
//! window's; and only a `Finished` event is a result.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use tidewise_helper_protocol::{
    Details, Event, MAX_DETAILS_BYTES, MAX_EVENT_LINE_BYTES, Outcome, parse_event,
};

use crate::helper_request::HelperRequest;

/// `pkexec`, by absolute path: no PATH lookup.
pub const PKEXEC_PATH: &str = "/usr/bin/pkexec";

/// Where the package installs the helper. polkit's action and the helper itself accept no other path (tied to the
/// policy and the PKGBUILD by a test below).
pub const HELPER_PATH: &str = "/usr/lib/tidewise/tidewise-helper";

/// How a helper run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEnd {
    /// The helper's own result.
    Finished(Outcome),
    /// `pkexec` exit 126: the user closed the password window.
    PromptClosed,
    /// `pkexec` exit 127: not authorised, authentication failed, or `pkexec` could not run it.
    NotAuthorised { stderr: String },
    /// The helper is not installed. Nothing was started.
    HelperMissing,
    /// Exit 2: the helper refused to start.
    HelperRefusedToStart { stderr: String },
    /// `pkexec` itself could not be started. Nothing was run.
    CouldNotStart { reason: String },
    /// No `Finished` event and no exit code that says what happened.
    NoResult {
        exit: Option<i32>,
        unreadable: u32,
        stderr: String,
    },
}

/// What was read from the helper's stdout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    /// The `Finished` event, if a whole one arrived.
    pub finished: Option<Outcome>,
    /// Lines that were not an event (too long, not JSON, cut short). Their text is never kept.
    pub unreadable: u32,
}

/// Reads the helper's stdout to its end. `on_event` gets every `Progress` and `Notice`; `Finished` is returned, and
/// whatever follows it is read and ignored. A line is never held longer than `MAX_EVENT_LINE_BYTES`.
pub fn read_events(mut reader: impl BufRead, mut on_event: impl FnMut(Event)) -> Reading {
    let mut reading = Reading::default();
    // One more than the cap, plus the new line.
    let limit = u64::try_from(MAX_EVENT_LINE_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.by_ref().take(limit).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return reading,
            Ok(_) => {}
        }
        let whole = line.last() == Some(&b'\n');
        if whole {
            line.pop();
        } else if line.len() > MAX_EVENT_LINE_BYTES {
            // Too long: throw the rest of it away. (A cut last line ends up here with nothing left to throw away.)
            skip_line(&mut reader);
        }
        if reading.finished.is_some() {
            continue;
        }
        let event = whole
            .then(|| std::str::from_utf8(&line).ok())
            .flatten()
            .and_then(|text| parse_event(text).ok());
        match event {
            Some(Event::Finished(outcome)) => reading.finished = Some(outcome),
            Some(event) => on_event(event),
            None => reading.unreadable = reading.unreadable.saturating_add(1),
        }
    }
}

/// Reads and drops everything up to and including the next new line.
fn skip_line(reader: &mut impl BufRead) {
    loop {
        let (used, done) = match reader.fill_buf() {
            Ok(buffer) if !buffer.is_empty() => match buffer.iter().position(|b| *b == b'\n') {
                Some(at) => (at + 1, true),
                None => (buffer.len(), false),
            },
            Err(error) if error.kind() == ErrorKind::Interrupted => (0, false),
            _ => return,
        };
        reader.consume(used);
        if done {
            return;
        }
    }
}

/// Reads stderr to its end, so the helper is never blocked, and keeps the start of it, cleaned like `Details`.
pub fn drain_capped(mut reader: impl Read) -> String {
    let mut kept = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                let room = MAX_DETAILS_BYTES.saturating_sub(kept.len());
                kept.extend(chunk.iter().take(count.min(room)));
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    Details::from_raw(&String::from_utf8_lossy(&kept))
        .as_str()
        .to_string()
}

/// What the end of a run means. A `Finished` event decides; without one, the exit code (`None`: a signal).
/// 126 and 127 are `pkexec`'s own (`pkexec(1)`, "Return value"); 2 is the helper's "refused to start".
pub fn job_end(reading: Reading, exit: Option<i32>, stderr: String) -> JobEnd {
    if let Some(outcome) = reading.finished {
        return JobEnd::Finished(outcome);
    }
    match exit {
        Some(126) => JobEnd::PromptClosed,
        Some(127) => JobEnd::NotAuthorised { stderr },
        Some(2) => JobEnd::HelperRefusedToStart { stderr },
        _ => JobEnd::NoResult {
            exit,
            unreadable: reading.unreadable,
            stderr,
        },
    }
}

/// The app's only program start. No shell, no PATH, an empty environment but for `LC_ALL=C`.
///
/// `--disable-internal-agent`: with no polkit agent running, `pkexec` would otherwise ask for the password on the
/// terminal the app was started from, and wait there behind the window (review R-02). Now it ends with 127.
/// `process_group(0)`: Ctrl+C, or closing that terminal, signals the terminal's foreground group; the root helper
/// must not be in it (review R-03, threat model question 16).
#[expect(
    clippy::disallowed_methods,
    reason = "the app's only program start: pkexec by absolute path, one fixed option, the helper's fixed path and one summary argument, no shell, environment cleared (golden rules 1, 3)"
)]
fn helper_command(summary: &str) -> Command {
    let mut command = Command::new(PKEXEC_PATH);
    command
        .arg("--disable-internal-agent")
        .arg(HELPER_PATH)
        .arg(summary)
        .process_group(0)
        .env_clear()
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Runs the helper once and waits for it. **Blocks** for as long as the password window is open and pacman runs, so
/// call it on a worker thread. There is no time limit and no stop: a transaction is never cut short from here.
pub fn run_install(request: HelperRequest, on_event: impl FnMut(Event)) -> JobEnd {
    // Asked before the password window, which would only end in an error.
    if !Path::new(HELPER_PATH).is_file() {
        return JobEnd::HelperMissing;
    }
    let mut child = match helper_command(&request.summary).spawn() {
        Ok(child) => child,
        Err(error) => {
            return JobEnd::CouldNotStart {
                reason: error.to_string(),
            };
        }
    };
    // Written from its own thread, then closed: a request can be bigger than a pipe holds, and the closed pipe is
    // how the helper knows it has all of it. A failed write shows up in the helper's own answer.
    let stdin = child.stdin.take();
    let json = request.json;
    let writer = std::thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(&json);
        }
    });
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || stderr.map(drain_capped).unwrap_or_default());
    let reading = child
        .stdout
        .take()
        .map(|stdout| read_events(BufReader::new(stdout), on_event))
        .unwrap_or_default();
    let exit = child.wait().ok().and_then(|status| status.code());
    let _ = writer.join();
    job_end(reading, exit, errors.join().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io::{BufReader, Cursor};

    use tidewise_helper_protocol::{Notice, Phase, Progress, event_line};

    use super::*;
    use tidewise_helper_protocol::ErrorKind;

    const OK: &str = include_str!("../../../tests/fixtures/helper-events/ok.jsonl");
    const FAILED: &str = include_str!("../../../tests/fixtures/helper-events/failed.jsonl");

    fn read(bytes: &[u8]) -> (Vec<Event>, Reading) {
        let mut events = Vec::new();
        // A small buffer, so lines arrive in pieces as they do from a pipe.
        let reading = read_events(BufReader::with_capacity(7, Cursor::new(bytes)), |event| {
            events.push(event)
        });
        (events, reading)
    }

    fn progress(current: u32) -> Event {
        Event::Progress(Progress {
            phase: Phase::Downloading,
            current,
            total: 10_000,
        })
    }

    fn line(event: &Event) -> String {
        format!("{}\n", event_line(event))
    }

    const FINISHED_OK: Option<Outcome> = Some(Outcome::Ok);

    #[test]
    fn progress_and_notices_are_passed_on_and_finished_is_the_result() {
        let (events, reading) = read(OK.as_bytes());
        assert_eq!(events.len(), 3);
        assert_eq!(events[2], Event::Notice(Notice::PackagesNotInstalled(1)));
        assert_eq!(
            reading,
            Reading {
                finished: FINISHED_OK,
                unreadable: 0
            }
        );
        let (_, failed) = read(FAILED.as_bytes());
        assert_eq!(
            failed.finished,
            Some(Outcome::Error(ErrorKind::PacmanFailed(Details::from_raw(
                "error: failed to commit transaction (conflicting files)"
            ))))
        );
    }

    #[test]
    fn a_line_that_is_too_long_is_skipped_and_the_next_one_is_read() {
        let mut bytes = vec![b'x'; 1024 * 1024];
        bytes.push(b'\n');
        bytes.extend(line(&progress(1)).bytes());
        bytes.extend(b"{\"finished\":\"ok\"}\n");
        let (events, reading) = read(&bytes);
        assert_eq!(events, [progress(1)]);
        assert_eq!(
            reading,
            Reading {
                finished: FINISHED_OK,
                unreadable: 1
            }
        );
    }

    #[test]
    fn a_line_of_exactly_the_cap_is_not_called_too_long_and_one_more_byte_is() {
        // Padded with spaces, which JSON ignores.
        let done = "{\"finished\":\"ok\"}";
        let at_cap = format!("{done}{}\n", " ".repeat(MAX_EVENT_LINE_BYTES - done.len()));
        assert_eq!(read(at_cap.as_bytes()).1.finished, FINISHED_OK);
        let over = format!(
            "{done}{}\n",
            " ".repeat(MAX_EVENT_LINE_BYTES - done.len() + 1)
        );
        let reading = read(over.as_bytes()).1;
        assert_eq!((reading.finished, reading.unreadable), (None, 1));
    }

    #[test]
    fn bytes_that_are_not_an_event_are_counted_and_never_passed_on() {
        let bytes = b"\0\0\0\n\xff\xfe\nhello\n{\"progress\":1}\n\n";
        let (events, reading) = read(bytes);
        assert!(events.is_empty());
        assert_eq!((reading.finished, reading.unreadable), (None, 5));
    }

    #[test]
    fn a_last_line_that_is_cut_short_is_not_a_result() {
        let (events, reading) = read(b"{\"finished\":\"ok\"}");
        assert!(events.is_empty());
        assert_eq!((reading.finished, reading.unreadable), (None, 1));
    }

    #[test]
    fn every_progress_line_of_a_long_job_is_passed_on() {
        let bytes: String = (0..10_000).map(|i| line(&progress(i))).collect();
        let (events, reading) = read(bytes.as_bytes());
        assert_eq!(events.len(), 10_000);
        assert_eq!(reading.unreadable, 0);
    }

    #[test]
    fn nothing_after_finished_is_listened_to() {
        let mut bytes = b"{\"finished\":\"ok\"}\n".to_vec();
        bytes.extend(line(&progress(1)).bytes());
        bytes.extend(b"junk\n{\"finished\":{\"error\":\"malformed\"}}\n");
        let (events, reading) = read(&bytes);
        assert!(events.is_empty());
        assert_eq!(
            reading,
            Reading {
                finished: FINISHED_OK,
                unreadable: 0
            }
        );
    }

    #[test]
    fn stderr_is_read_to_its_end_but_only_its_cleaned_start_is_kept() {
        let mut bytes = b"refused\x1b[31m\nto start\n".to_vec();
        bytes.extend(vec![b'y'; 100_000]);
        let mut reader = Cursor::new(bytes);
        let kept = drain_capped(&mut reader);
        assert_eq!(reader.position(), reader.get_ref().len() as u64);
        assert!(kept.starts_with("refused[31m\nto start\nyyy"), "{kept:?}");
        assert!(kept.len() <= MAX_DETAILS_BYTES);
    }

    #[test]
    fn the_finished_event_decides_and_the_exit_code_only_without_one() {
        let end = |finished: Option<Outcome>, exit: Option<i32>| {
            job_end(
                Reading {
                    finished,
                    unreadable: 2,
                },
                exit,
                "words".to_string(),
            )
        };
        let words = || "words".to_string();
        let refused = Outcome::Error(ErrorKind::TransactionChanged);
        // Whatever the exit code says.
        for exit in [Some(0), Some(1), Some(126), Some(127), Some(2), None] {
            assert_eq!(
                end(FINISHED_OK, exit),
                JobEnd::Finished(Outcome::Ok),
                "{exit:?}"
            );
            assert_eq!(
                end(Some(refused.clone()), exit),
                JobEnd::Finished(refused.clone()),
                "{exit:?}"
            );
        }
        assert_eq!(end(None, Some(126)), JobEnd::PromptClosed);
        assert_eq!(
            end(None, Some(127)),
            JobEnd::NotAuthorised { stderr: words() }
        );
        assert_eq!(
            end(None, Some(2)),
            JobEnd::HelperRefusedToStart { stderr: words() }
        );
        for exit in [Some(0), Some(1), Some(101), None] {
            assert_eq!(
                end(None, exit),
                JobEnd::NoResult {
                    exit,
                    unreadable: 2,
                    stderr: words()
                },
                "{exit:?}"
            );
        }
    }

    #[test]
    fn pkexec_starts_by_absolute_path_without_its_own_agent_and_in_its_own_group() {
        let command = helper_command("install extra/sl (1 packages)");
        assert_eq!(command.get_program(), OsStr::new("/usr/bin/pkexec"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--disable-internal-agent",
                HELPER_PATH,
                "install extra/sl (1 packages)"
            ]
        );
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            [(OsStr::new("LC_ALL"), Some(OsStr::new("C")))]
        );
        assert_eq!(command.get_current_dir(), Some(std::path::Path::new("/")));
        // `get_envs` cannot say whether the rest of the environment is dropped; std's Debug output writes `env -i`
        // for a cleared one.
        let shown = format!("{command:?}");
        assert!(shown.contains("env -i "), "{shown}");
        // No getter for the process group either: the long Debug form names it. 0 = a new group of its own.
        let fields: String = format!("{command:#?}").split_whitespace().collect();
        assert!(fields.contains("pgroup:Some(0,)"), "{fields}");
    }

    #[test]
    fn the_helper_path_is_the_one_the_policy_names_and_the_package_installs() {
        let policy = include_str!("../../../packaging/polkit/io.github.tidewise.helper.policy");
        let annotation = format!("org.freedesktop.policykit.exec.path\">{HELPER_PATH}</annotate>");
        assert!(policy.contains(&annotation));
        let recipe = include_str!("../../../packaging/aur/PKGBUILD");
        let installed = format!("target/release/tidewise-helper \"$pkgdir{HELPER_PATH}\"");
        assert!(recipe.contains(&installed));
    }
}
