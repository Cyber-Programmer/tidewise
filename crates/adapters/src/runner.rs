//! Running a program, behind a trait so tests can use a fake. The real runner starts the program by
//! absolute path with an argument list (never through a shell) and `LC_ALL=C`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner {
    /// `program` must be an absolute path.
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String>;
}

/// Starts real programs.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        let _ = (program, args);
        Err("not implemented".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_runner_passes_arguments_as_a_list_and_forces_the_c_locale() {
        let out = SystemRunner
            .run("/usr/bin/printf", &["%s|", "a b", "$(id)", "; x"])
            .unwrap();
        assert!(out.success);
        assert_eq!(out.stdout, "a b|$(id)|; x|");
        let env = SystemRunner.run("/usr/bin/env", &[]).unwrap();
        assert!(env.stdout.lines().any(|l| l == "LC_ALL=C"));
    }

    #[test]
    fn failures_and_relative_programs_are_reported() {
        let out = SystemRunner.run("/usr/bin/false", &[]).unwrap();
        assert!(!out.success);
        assert!(
            SystemRunner.run("true", &[]).is_err(),
            "relative paths are refused"
        );
        assert!(SystemRunner.run("/nonexistent/prog", &[]).is_err());
    }
}
