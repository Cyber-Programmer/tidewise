//! Test support: a scripted `Runner` that answers each command with the next prepared result and remembers what was
//! run. Used by the flow tests and the `run` tests. Compiled only for tests.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::lists::{ListsError, MoveError};
use crate::pacman_args::PacmanCommand;
use crate::runner::{RunOutput, Runner, RunnerError};

/// Answers each command with the next scripted result and remembers what was run.
pub struct Fake {
    pub lock: bool,
    pub script: VecDeque<Result<RunOutput, RunnerError>>,
    pub ran: Vec<PacmanCommand>,
    /// The private `db.lck` is "there".
    pub private_lock: bool,
    /// What `prepare_private_lists` answers, and how often it was asked.
    pub prepare_result: Result<(), ListsError>,
    pub prepared: u32,
    /// What `move_lists_in` answers; `moved_after` is how many commands had run when it was asked (None: never asked).
    pub move_result: Result<(), MoveError>,
    pub moved_after: Option<usize>,
}

impl Fake {
    pub fn new(script: Vec<Result<RunOutput, RunnerError>>) -> Fake {
        Fake {
            lock: false,
            script: script.into(),
            ran: Vec::new(),
            private_lock: false,
            prepare_result: Ok(()),
            prepared: 0,
            move_result: Ok(()),
            moved_after: None,
        }
    }
}

impl Runner for Fake {
    fn run(
        &mut self,
        command: &PacmanCommand,
        on_stdout_line: &mut dyn FnMut(&str),
    ) -> Result<RunOutput, RunnerError> {
        self.ran.push(command.clone());
        let answer = self.script.pop_front().unwrap_or(Err(RunnerError::Io))?;
        for line in answer.stdout.split('\n') {
            on_stdout_line(line);
        }
        Ok(answer)
    }

    fn database_lock_present(&self) -> bool {
        self.lock
    }

    fn prepare_private_lists(&mut self) -> Result<(), ListsError> {
        self.prepared += 1;
        self.prepare_result
    }

    fn private_lock_present(&self) -> bool {
        self.private_lock
    }

    fn move_lists_in(&mut self) -> Result<(), MoveError> {
        self.moved_after = Some(self.ran.len());
        self.move_result
    }
}

pub fn out(exit: i32, stdout: &str, stderr: &str) -> Result<RunOutput, RunnerError> {
    Ok(RunOutput {
        exit: Some(exit),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        timed_out: false,
        truncated: false,
    })
}

pub fn ok(stdout: &str) -> Result<RunOutput, RunnerError> {
    out(0, stdout, "")
}

/// A throwaway folder under the system temp dir, removed when dropped. (No `tempfile` crate: this crate has one
/// dependency.) The one TempDir of this crate's unit tests, so the only place they delete a folder. The counter makes
/// every folder unique inside a process, whatever the test's name.
pub struct TempDir(pub PathBuf);

impl TempDir {
    #[expect(
        clippy::disallowed_methods,
        reason = "the unit tests' one TempDir removes a stale folder of its own name before it creates it"
    )]
    pub fn new(test: &str) -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let number = COUNT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tidewise-helper-test-{}-{number}-{test}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    #[expect(
        clippy::disallowed_methods,
        reason = "the unit tests' one TempDir removes its own folder when the test ends"
    )]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
