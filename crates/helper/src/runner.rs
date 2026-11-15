//! Running pacman (threats T-04, T-08, T-10, T-13, T-17; golden rules 3, 4, 9, 10).
//!
//! `/usr/bin/pacman` by absolute path, the argument list from `pacman_args` (names after `--`), stdin null, no shell,
//! and an environment that is neither empty nor the caller's: `env_clear` plus the fixed pair from `env`.
//!
//! **Output goes to files, never to pipes.** Measured in the test container: when the reader of a pipe goes away,
//! pacman is killed by SIGPIPE at its next write and leaves a stale `db.lck`. The files are in the verified work folder
//! (`workdir`); this module reads them as they grow (progress) and after the run (parsing), keeps at most a fixed
//! amount, and removes them at the end.
//!
//! **Own process group** for the child (`CommandExt::process_group(0)`, safe): a terminal's SIGINT or SIGHUP, sent to
//! the helper's group, then never reaches pacman. Measured: SIGHUP in the middle of a commit makes pacman stop after
//! the current package with exit status 0, a half transaction that looks like success.
//!
//! Timeouts only on reads and dry runs (they hold no lock, T-17). A real transaction is never killed here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::pacman_args::{ArgsRefused, PacmanCommand};
use crate::workdir::{WorkDir, WorkDirError};

/// Dry runs and reads that take longer than this are killed.
pub const READ_TIMEOUT: Duration = Duration::from_secs(120);
/// Most stdout kept per run, in bytes (a 5 000-line `--print` is well under 1 MiB).
pub const STDOUT_CAP: usize = 4 * 1024 * 1024;
/// Most stderr kept per run, in bytes.
pub const STDERR_CAP: usize = 64 * 1024;
/// A line longer than this is delivered in pieces.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// pacman's database lock. Only ever looked at, never created, changed or removed (golden rule 9).
pub const DATABASE_LOCK: &str = "/var/lib/pacman/db.lck";

/// What one pacman run left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// The exit status, or `None` if pacman was killed (by a signal, or by the timeout).
    pub exit: Option<i32>,
    /// What pacman wrote to stdout, up to the cap, with invalid UTF-8 replaced.
    pub stdout: String,
    /// What pacman wrote to stderr, up to the cap, with invalid UTF-8 replaced.
    pub stderr: String,
    /// The run was killed because it took longer than the timeout.
    pub timed_out: bool,
    /// More output was written than the cap kept.
    pub truncated: bool,
}

/// Why pacman could not be run at all. Codes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerError {
    /// No command line could be built (bad name, no packages).
    Args(ArgsRefused),
    /// The work folder could not be used.
    WorkDir(WorkDirError),
    /// The program could not be started.
    Spawn,
    /// Reading the output failed.
    Io,
}

/// What the jobs need: run a pacman command (feeding stdout lines as they appear), and look at the lock.
pub trait Runner {
    fn run(
        &mut self,
        command: &PacmanCommand,
        on_stdout_line: &mut dyn FnMut(&str),
    ) -> Result<RunOutput, RunnerError>;

    /// True if `db.lck` exists (or cannot be checked: unknown is treated as present).
    fn database_lock_present(&self) -> bool;
}

/// Cuts a byte stream into lines at `\n` only, as chunks arrive.
#[derive(Debug, Default)]
pub struct LineBuffer {
    partial: Vec<u8>,
}

impl LineBuffer {
    pub fn new() -> Self {
        LineBuffer::default()
    }

    /// Feeds a chunk; calls `on_line` for every complete line (without the `\n`).
    pub fn push(&mut self, chunk: &[u8], on_line: &mut dyn FnMut(&str)) {
        let _ = (chunk, on_line, &self.partial, MAX_LINE_BYTES);
    }

    /// The last line, if the stream ended without a `\n`.
    pub fn finish(&mut self, on_line: &mut dyn FnMut(&str)) {
        let _ = on_line;
    }
}

/// The time limit of a command: reads and dry runs have one, real transactions never.
pub fn timeout_for(command: &PacmanCommand) -> Option<Duration> {
    let _ = command;
    None
}

/// Runs the real pacman.
#[derive(Debug)]
pub struct PacmanRunner {
    workdir: WorkDir,
    serial: u32,
    program: PathBuf,
    read_timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
    lock_path: PathBuf,
}

impl PacmanRunner {
    /// The real thing: `/usr/bin/pacman`, the real limits, the real lock path.
    pub fn new(workdir: WorkDir) -> Self {
        PacmanRunner {
            workdir,
            serial: 0,
            program: PathBuf::new(),
            read_timeout: Duration::ZERO,
            stdout_cap: 0,
            stderr_cap: 0,
            lock_path: PathBuf::new(),
        }
    }

    /// The program and its arguments, with an optional time limit. `run` calls this with pacman.
    fn run_program(
        &mut self,
        program: &Path,
        args: &[String],
        timeout: Option<Duration>,
        on_stdout_line: &mut dyn FnMut(&str),
    ) -> Result<RunOutput, RunnerError> {
        let _ = (
            program,
            args,
            timeout,
            on_stdout_line,
            &self.workdir,
            self.serial,
        );
        Err(RunnerError::Io)
    }
}

impl Runner for PacmanRunner {
    fn run(
        &mut self,
        command: &PacmanCommand,
        on_stdout_line: &mut dyn FnMut(&str),
    ) -> Result<RunOutput, RunnerError> {
        let _ = (command, on_stdout_line, &self.program, self.read_timeout);
        Err(RunnerError::Io)
    }

    fn database_lock_present(&self) -> bool {
        let _ = (&self.lock_path, self.stdout_cap, self.stderr_cap);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use tidewise_helper_protocol::PackageRef;

    // ---- a work folder for the tests and a runner pointed at a stand-in program ---------------------------

    struct Temp(PathBuf);

    impl Temp {
        fn new(test: &str) -> Temp {
            let path =
                std::env::temp_dir().join(format!("tidewise-runner-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Temp(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn me() -> u32 {
        fs::metadata("/proc/self").unwrap().uid()
    }

    fn runner_in(temp: &Temp) -> PacmanRunner {
        let workdir = WorkDir::open(&temp.0.join("work"), me()).unwrap();
        let mut runner = PacmanRunner::new(workdir);
        runner.lock_path = temp.0.join("db.lck");
        runner
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn go(
        runner: &mut PacmanRunner,
        program: &str,
        args: &[&str],
        timeout: Option<Duration>,
    ) -> RunOutput {
        runner
            .run_program(Path::new(program), &strings(args), timeout, &mut |_| {})
            .unwrap()
    }

    fn files_left(temp: &Temp) -> Vec<String> {
        fs::read_dir(temp.0.join("work"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
    }

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    // ---- the real defaults --------------------------------------------------------------------------------

    #[test]
    fn the_real_runner_uses_pacman_by_absolute_path_and_the_real_limits() {
        let temp = Temp::new("defaults");
        let runner = runner_in(&temp);
        assert_eq!(runner.program, Path::new("/usr/bin/pacman"));
        assert_eq!(runner.read_timeout, READ_TIMEOUT);
        assert_eq!(runner.stdout_cap, STDOUT_CAP);
        assert_eq!(runner.stderr_cap, STDERR_CAP);
        assert_eq!(DATABASE_LOCK, "/var/lib/pacman/db.lck");
    }

    #[test]
    fn reads_and_dry_runs_have_a_time_limit_and_real_transactions_never_do() {
        let list = vec![pkg("extra", "vim")];
        for command in [
            PacmanCommand::PendingUpdates,
            PacmanCommand::DryRunInstall(list.clone()),
            PacmanCommand::DryRunUpdate(vec![]),
            PacmanCommand::VerifyInstalled(list.clone()),
        ] {
            assert_eq!(timeout_for(&command), Some(READ_TIMEOUT), "{command:?}");
        }
        for command in [
            PacmanCommand::Sync,
            PacmanCommand::RealInstall(list.clone()),
            PacmanCommand::RealUpdate(vec![]),
            PacmanCommand::MarkAsDeps(list),
        ] {
            assert_eq!(timeout_for(&command), None, "{command:?}");
        }
    }

    // ---- how the child is started -------------------------------------------------------------------------

    #[test]
    fn stdout_and_stderr_are_files_in_the_work_folder_not_pipes() {
        let temp = Temp::new("files");
        let mut runner = runner_in(&temp);
        let out = go(&mut runner, "/usr/bin/readlink", &["/proc/self/fd/1"], None);
        let work = temp.0.join("work");
        assert!(
            out.stdout.starts_with(work.to_str().unwrap()),
            "{}",
            out.stdout
        );
        assert!(out.stdout.trim_end().ends_with(".out"), "{}", out.stdout);
        let err = go(&mut runner, "/usr/bin/readlink", &["/proc/self/fd/2"], None);
        assert!(
            err.stdout.starts_with(work.to_str().unwrap()),
            "{}",
            err.stdout
        );
        assert!(err.stdout.trim_end().ends_with(".err"), "{}", err.stdout);
    }

    #[test]
    fn the_child_leads_its_own_process_group() {
        let temp = Temp::new("group");
        let mut runner = runner_in(&temp);
        let out = go(&mut runner, "/usr/bin/cat", &["/proc/self/stat"], None);
        // "pid (comm) state ppid pgrp ..."
        let pid = out.stdout.split(' ').next().unwrap().to_string();
        let after_name = out.stdout.rsplit_once(") ").unwrap().1;
        let pgrp = after_name.split(' ').nth(2).unwrap();
        assert_eq!(pgrp, pid, "{}", out.stdout);
        assert_ne!(pgrp, std::process::id().to_string());
    }

    #[test]
    fn the_child_gets_exactly_the_fixed_environment() {
        let temp = Temp::new("env");
        let mut runner = runner_in(&temp);
        let out = go(&mut runner, "/usr/bin/env", &[], None);
        let mut lines: Vec<&str> = out.stdout.lines().collect();
        lines.sort_unstable();
        assert_eq!(lines, vec!["LC_ALL=C", "PATH=/usr/bin"]);
    }

    #[test]
    fn stdin_is_null_so_a_program_that_reads_it_sees_the_end_at_once() {
        let temp = Temp::new("stdin");
        let mut runner = runner_in(&temp);
        let out = go(
            &mut runner,
            "/usr/bin/cat",
            &[],
            Some(Duration::from_secs(20)),
        );
        assert_eq!((out.exit, out.timed_out), (Some(0), false));
        assert_eq!(out.stdout, "");
    }

    // ---- results ------------------------------------------------------------------------------------------

    #[test]
    fn the_exit_status_is_reported() {
        let temp = Temp::new("exit");
        let mut runner = runner_in(&temp);
        assert_eq!(go(&mut runner, "/usr/bin/true", &[], None).exit, Some(0));
        assert_eq!(go(&mut runner, "/usr/bin/false", &[], None).exit, Some(1));
    }

    #[test]
    fn stdout_and_stderr_are_kept_apart() {
        let temp = Temp::new("apart");
        let file = temp.0.join("input.txt");
        fs::write(&file, "from the file\n").unwrap();
        let mut runner = runner_in(&temp);
        let out = go(
            &mut runner,
            "/usr/bin/cat",
            &[file.to_str().unwrap(), "/nonexistent-tidewise"],
            None,
        );
        assert_eq!(out.stdout, "from the file\n");
        assert!(out.stderr.contains("No such file"), "{}", out.stderr);
        assert_eq!(out.exit, Some(1));
    }

    #[test]
    fn invalid_utf8_becomes_replacement_characters() {
        let temp = Temp::new("utf8");
        let mut runner = runner_in(&temp);
        let out = go(&mut runner, "/usr/bin/printf", &["\\377abc"], None);
        assert_eq!(out.stdout, "\u{fffd}abc");
    }

    #[test]
    fn stdout_lines_are_delivered_in_order_including_a_last_line_without_a_new_line() {
        let temp = Temp::new("lines");
        let mut runner = runner_in(&temp);
        let mut seen: Vec<String> = Vec::new();
        runner
            .run_program(
                Path::new("/usr/bin/printf"),
                &strings(&["a\\nb\\n\\nc"]),
                None,
                &mut |line| seen.push(line.to_string()),
            )
            .unwrap();
        assert_eq!(seen, vec!["a", "b", "", "c"]);
    }

    // ---- limits -------------------------------------------------------------------------------------------

    #[test]
    fn a_read_that_runs_too_long_is_killed() {
        let temp = Temp::new("timeout");
        let mut runner = runner_in(&temp);
        let started = std::time::Instant::now();
        let out = go(
            &mut runner,
            "/usr/bin/sleep",
            &["30"],
            Some(Duration::from_millis(200)),
        );
        assert!(out.timed_out);
        assert_eq!(out.exit, None);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_run_without_a_limit_is_left_to_finish() {
        let temp = Temp::new("nolimit");
        let mut runner = runner_in(&temp);
        runner.read_timeout = Duration::from_millis(50);
        let out = go(&mut runner, "/usr/bin/sleep", &["1"], None);
        assert_eq!((out.exit, out.timed_out), (Some(0), false));
    }

    #[test]
    fn output_past_the_cap_is_dropped_and_said_so() {
        let temp = Temp::new("cap");
        let mut runner = runner_in(&temp);
        runner.stdout_cap = 1000;
        let mut longest = 0;
        let mut lines = 0;
        let out = runner
            .run_program(
                Path::new("/usr/bin/head"),
                &strings(&["-c", "300000", "/dev/zero"]),
                None,
                &mut |line| {
                    longest = longest.max(line.len());
                    lines += 1;
                },
            )
            .unwrap();
        assert!(out.truncated);
        assert!(out.stdout.len() <= 1000);
        assert!(lines >= 1);
        assert!(longest <= MAX_LINE_BYTES * 3, "{longest}");
    }

    #[test]
    fn small_output_is_not_marked_truncated() {
        let temp = Temp::new("small");
        let mut runner = runner_in(&temp);
        assert!(!go(&mut runner, "/usr/bin/echo", &["hi"], None).truncated);
    }

    // ---- cleanup and refusals -----------------------------------------------------------------------------

    #[test]
    fn the_files_are_removed_after_success_failure_and_timeout() {
        let temp = Temp::new("cleanup");
        let mut runner = runner_in(&temp);
        go(&mut runner, "/usr/bin/echo", &["x"], None);
        go(&mut runner, "/usr/bin/false", &[], None);
        go(
            &mut runner,
            "/usr/bin/sleep",
            &["30"],
            Some(Duration::from_millis(100)),
        );
        assert_eq!(files_left(&temp), Vec::<String>::new());
    }

    #[test]
    fn a_program_that_cannot_start_is_a_spawn_error_and_leaves_nothing() {
        let temp = Temp::new("spawn");
        let mut runner = runner_in(&temp);
        let result = runner.run_program(Path::new("/nonexistent/pacman"), &[], None, &mut |_| {});
        assert_eq!(result.unwrap_err(), RunnerError::Spawn);
        assert_eq!(files_left(&temp), Vec::<String>::new());
    }

    #[test]
    fn a_bad_command_is_refused_before_anything_is_created_or_run() {
        let temp = Temp::new("refused");
        let mut runner = runner_in(&temp);
        let bad = PacmanCommand::RealInstall(vec![pkg("extra", "--force")]);
        assert_eq!(
            runner.run(&bad, &mut |_| {}).unwrap_err(),
            RunnerError::Args(ArgsRefused::BadName)
        );
        let none = PacmanCommand::RealInstall(vec![]);
        assert_eq!(
            runner.run(&none, &mut |_| {}).unwrap_err(),
            RunnerError::Args(ArgsRefused::NoTargets)
        );
        assert_eq!(files_left(&temp), Vec::<String>::new());
    }

    #[test]
    fn run_builds_the_command_line_and_runs_the_program() {
        // echo is not pacman, but it shows exactly what the argument list was.
        let temp = Temp::new("plumbing");
        let mut runner = runner_in(&temp);
        runner.program = PathBuf::from("/usr/bin/echo");
        let out = runner
            .run(&PacmanCommand::PendingUpdates, &mut |_| {})
            .unwrap();
        assert_eq!(out.stdout, "-Qu\n");
        let out = runner
            .run(
                &PacmanCommand::VerifyInstalled(vec![pkg("extra", "vim")]),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(out.stdout, "-Q -- vim\n");
    }

    #[test]
    fn two_runs_in_a_row_do_not_collide() {
        let temp = Temp::new("serial");
        let mut runner = runner_in(&temp);
        for _ in 0..5 {
            assert_eq!(go(&mut runner, "/usr/bin/true", &[], None).exit, Some(0));
        }
    }

    // ---- the lock is only looked at -----------------------------------------------------------------------

    #[test]
    fn the_lock_is_present_when_the_file_is_and_never_touched() {
        let temp = Temp::new("lock");
        let runner = runner_in(&temp);
        assert!(!runner.database_lock_present());
        fs::write(temp.0.join("db.lck"), b"").unwrap();
        let before = fs::metadata(temp.0.join("db.lck")).unwrap();
        assert!(runner.database_lock_present());
        let after = fs::metadata(temp.0.join("db.lck")).unwrap();
        assert_eq!(
            (before.ino(), before.mtime(), before.mtime_nsec()),
            (after.ino(), after.mtime(), after.mtime_nsec())
        );
    }

    #[test]
    fn a_lock_that_is_a_symlink_or_a_folder_counts_as_present() {
        let temp = Temp::new("lock-odd");
        let runner = runner_in(&temp);
        std::os::unix::fs::symlink("/nonexistent", temp.0.join("db.lck")).unwrap();
        assert!(runner.database_lock_present());
        fs::remove_file(temp.0.join("db.lck")).unwrap();
        fs::create_dir(temp.0.join("db.lck")).unwrap();
        assert!(runner.database_lock_present());
    }

    // ---- LineBuffer ---------------------------------------------------------------------------------------

    fn lines_of(chunks: &[&[u8]]) -> Vec<String> {
        let mut buffer = LineBuffer::new();
        let mut seen = Vec::new();
        for chunk in chunks {
            buffer.push(chunk, &mut |line| seen.push(line.to_string()));
        }
        buffer.finish(&mut |line| seen.push(line.to_string()));
        seen
    }

    #[test]
    fn lines_are_found_across_chunk_boundaries() {
        assert_eq!(
            lines_of(&[b"ab", b"c\nde", b"f\n", b"g"]),
            vec!["abc", "def", "g"]
        );
        assert_eq!(lines_of(&[b"a\n\nb\n"]), vec!["a", "", "b"]);
        assert_eq!(lines_of(&[b""]), Vec::<String>::new());
        assert_eq!(lines_of(&[b"\n"]), vec![""]);
    }

    #[test]
    fn only_a_new_line_ends_a_line_a_carriage_return_stays() {
        assert_eq!(lines_of(&[b"a\r\nb"]), vec!["a\r", "b"]);
    }

    #[test]
    fn invalid_utf8_in_a_line_is_replaced() {
        assert_eq!(lines_of(&[b"a\xffb\n"]), vec!["a\u{fffd}b"]);
    }

    #[test]
    fn a_line_without_an_end_is_delivered_in_pieces_not_stored_forever() {
        let big = vec![b'x'; MAX_LINE_BYTES * 3 + 10];
        let seen = lines_of(&[&big]);
        assert!(seen.len() >= 3, "{}", seen.len());
        assert!(
            seen.iter().all(|line| line.len() <= MAX_LINE_BYTES),
            "pieces are capped"
        );
        assert_eq!(
            seen.iter().map(String::len).sum::<usize>(),
            big.len(),
            "nothing is lost"
        );
    }
}
