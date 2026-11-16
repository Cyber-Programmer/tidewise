//! The `tidewise-helper` binary. It only gathers the real facts and hands them to `tidewise_helper::run::run`, so
//! everything that matters can be tested without being root. Same code rules as the library (see `lib.rs`).
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::print_stdout,
        clippy::print_stderr,
        clippy::dbg_macro
    )
)]

use std::path::Path;
use std::process::ExitCode;

use tidewise_helper::env::set_umask_022;
use tidewise_helper::fds::read_fd_entries;
use tidewise_helper::flows::work_folder_unusable;
use tidewise_helper::journal::{JOURNAL_SOCKET, send};
use tidewise_helper::run::{Facts, run};
use tidewise_helper::runner::{PacmanRunner, Runner};
use tidewise_helper::uid::read_status;
use tidewise_helper::workdir::{WorkDir, another_process_is_alive};
use tidewise_helper_protocol::ErrorKind;

fn main() -> ExitCode {
    // Before anything is created: pkexec passes on the caller's umask and pacman does not fix it (see env.rs).
    set_umask_022();

    // The only things read from the process. The environment is read for one variable, PKEXEC_UID, and nothing
    // else. `chdir("/")` happens here, before the root check inside `run`: it only changes our own working
    // directory, so it is harmless even when a later check refuses.
    let facts = Facts {
        fds: read_fd_entries(),
        own_pid: std::process::id(),
        status: read_status(),
        chdir: std::env::set_current_dir("/"),
        args: std::env::args_os().collect(),
        pkexec_uid: std::env::var_os("PKEXEC_UID"),
    };

    // A journal that cannot be reached changes neither the outcome nor the exit code (threat model T-14).
    let mut log = |datagram: &[u8]| send(Path::new(JOURNAL_SOCKET), datagram).is_ok();
    // The work folder and the runner are made only once a request has been accepted. The folder belongs to root
    // (uid 0, which is what the helper is by now); leftovers of a killed earlier helper are removed first.
    let mut make_runner = || -> Result<Box<dyn Runner>, ErrorKind> {
        let dir = WorkDir::open_default().map_err(|_| work_folder_unusable())?;
        dir.remove_leftovers(&another_process_is_alive);
        Ok(Box::new(PacmanRunner::new(dir)))
    };

    let code = run(
        facts,
        std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        &mut log,
        &mut make_runner,
    );
    ExitCode::from(code)
}
