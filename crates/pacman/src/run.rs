//! Runs pacman (read-only) and turns its output into values.
//!
//! This is the only module that starts a process. The work is split in two so it can be tested
//! without pacman: "run the process" (`run`) and "turn its `Output` into the result"
//! (`packages_from_output`).

use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::{InstalledPackage, PacmanError, parse_qi};

/// Absolute path: never looked up through `PATH`.
const PACMAN: &str = "/usr/bin/pacman";

/// Builds the command: absolute program, arguments as a list (no shell), empty environment
/// except `LC_ALL=C`, and no stdin so pacman can never wait for a prompt.
fn pacman_command(program: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    command
}

/// A non-zero exit (or a signal) is an error that keeps pacman's own words.
fn check_exit(output: &Output) -> Result<(), PacmanError> {
    if output.status.success() {
        return Ok(());
    }
    Err(PacmanError::Failed {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// Turns what pacman printed into packages. Pure: takes an `Output`, runs nothing.
fn packages_from_output(output: &Output) -> Result<Vec<InstalledPackage>, PacmanError> {
    check_exit(output)?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_qi(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const MULTI: &str = include_str!("../../../tests/fixtures/pacman/qi-multi.txt");

    /// A fake pacman result. `code` is the exit code (0 = success).
    fn fake_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    /// Runs `packages_from_output` and returns its error, with a clear message if it succeeded.
    fn expect_error(output: &Output) -> PacmanError {
        match packages_from_output(output) {
            Err(error) => error,
            Ok(packages) => panic!("expected an error, got {} packages", packages.len()),
        }
    }

    #[test]
    fn success_output_parses_like_parse_qi() {
        let output = fake_output(0, MULTI.as_bytes(), b"");
        let packages = packages_from_output(&output).unwrap();
        assert_eq!(packages, parse_qi(MULTI).unwrap());
        assert!(!packages.is_empty());
    }

    #[test]
    fn command_is_absolute_pacman_with_arg_list_and_c_locale() {
        let cmd = pacman_command(Path::new(PACMAN), &["-Qi"]);
        assert_eq!(cmd.get_program(), "/usr/bin/pacman");
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, ["-Qi"]);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert_eq!(envs, [(OsStr::new("LC_ALL"), Some(OsStr::new("C")))]);
    }

    #[test]
    fn non_zero_exit_keeps_stderr() {
        let text = "error: package 'x' was not found\n";
        let output = fake_output(1, b"", text.as_bytes());
        let error = expect_error(&output);
        assert!(
            matches!(&error, PacmanError::Failed { code: Some(1), stderr }
                if stderr == "error: package 'x' was not found"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("error: package 'x' was not found"));
        assert!(error.to_string().contains("1"));
    }

    #[test]
    fn exit_by_signal_has_no_code() {
        // Raw wait status 9 = "killed by signal 9".
        let output = Output {
            status: ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: b"partial".to_vec(),
        };
        let error = expect_error(&output);
        assert!(
            matches!(&error, PacmanError::Failed { code: None, stderr } if stderr == "partial"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("signal"));
    }
}
