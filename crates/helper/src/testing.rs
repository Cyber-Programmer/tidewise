//! Test support: a scripted `Runner` that answers each command with the next prepared result and remembers what was
//! run. Used by the flow tests and the `run` tests. Compiled only for tests.

use std::collections::VecDeque;

use crate::pacman_args::PacmanCommand;
use crate::runner::{RunOutput, Runner, RunnerError};

/// Answers each command with the next scripted result and remembers what was run.
pub struct Fake {
    pub lock: bool,
    pub script: VecDeque<Result<RunOutput, RunnerError>>,
    pub ran: Vec<PacmanCommand>,
}

impl Fake {
    pub fn new(script: Vec<Result<RunOutput, RunnerError>>) -> Fake {
        Fake {
            lock: false,
            script: script.into(),
            ran: Vec::new(),
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
