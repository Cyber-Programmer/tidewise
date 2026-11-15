//! The only pacman command lines the helper can build (threats T-04, T-05, T-11; golden rules 3, 4, 10).
//!
//! One template per `PacmanCommand`. Everything before `--` is fixed text; the only caller-supplied values are
//! package names, placed after `--`, after being checked once more against the same allow-lists the protocol used.
//! There is no free-form option anywhere: a request cannot add `--root`, `--overwrite`, `-d`, `-U` or anything else.
//!
//! `--noconfirm` and `--ask 66` appear only on real (non-dry-run) transactions, never on a read. `--ask 66` is
//! 2 + 64 from `alpm_question_type_t` (`/usr/include/alpm.h`): it inverts the default answer of "replace this
//! package?" (which pacman answers yes, and a replacement REMOVES the old package) and of "import this PGP key?" (yes
//! by default under `--noconfirm`), so both become "no". See `docs/decisions/0006-pacman-noconfirm-ask.md`.
//! `-y` appears only in `-Sy`, which is run once per update, never before a plain install (golden rule 4).

use tidewise_helper_protocol::PackageRef;
use tidewise_names::{is_helper_repo_name, is_valid_package_name};

/// pacman, by absolute path (no `PATH` lookup, no shell).
pub const PACMAN: &str = "/usr/bin/pacman";

/// What the helper may ask pacman to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacmanCommand {
    /// `pacman -Qu`: pending updates (a read).
    PendingUpdates,
    /// `pacman -Sy`: refresh the package lists. Once per update, never before a plain install.
    Sync,
    /// `pacman -S --print ...`: what installing these would do (a read).
    DryRunInstall(Vec<PackageRef>),
    /// `pacman -Su --print ...`: what a full update (plus these, if any) would do (a read).
    DryRunUpdate(Vec<PackageRef>),
    /// `pacman -S --noconfirm --ask 66 -- ...`: the real install.
    RealInstall(Vec<PackageRef>),
    /// `pacman -Su --noconfirm --ask 66 [-- ...]`: the real update (plus these, if any). No `-y`.
    RealUpdate(Vec<PackageRef>),
    /// `pacman -D --asdeps -- ...`: mark these (the chosen providers) as installed as dependencies.
    MarkAsDeps(Vec<PackageRef>),
    /// `pacman -Q`: every installed package and version (a read, fixed argv, no names). Taken before and after a real
    /// run: the difference shows what disappeared.
    ListInstalled,
}

/// Why no command line was built. A code only; the offending text is never repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgsRefused {
    /// A repository or package name failed the allow-list.
    BadName,
    /// The command needs at least one package and has none.
    NoTargets,
}

/// The argument list (without the program name) for `command`.
pub fn args(command: &PacmanCommand) -> Result<Vec<String>, ArgsRefused> {
    let dry_run = ["--print", "--print-format", "%r/%n %v"];
    let real = ["--noconfirm", "--ask", "66"];
    match command {
        PacmanCommand::PendingUpdates => Ok(fixed(&["-Qu"])),
        PacmanCommand::Sync => Ok(fixed(&["-Sy"])),
        PacmanCommand::DryRunInstall(packages) => {
            build(&["-S"], &dry_run, repo_and_name(packages)?, Names::Required)
        }
        PacmanCommand::DryRunUpdate(packages) => build(
            &["-Su"],
            &dry_run,
            repo_and_name(packages)?,
            Names::Optional,
        ),
        PacmanCommand::RealInstall(packages) => {
            build(&["-S"], &real, repo_and_name(packages)?, Names::Required)
        }
        PacmanCommand::RealUpdate(packages) => {
            build(&["-Su"], &real, repo_and_name(packages)?, Names::Optional)
        }
        PacmanCommand::MarkAsDeps(packages) => build(
            &["-D"],
            &["--asdeps"],
            name_only(packages)?,
            Names::Required,
        ),
        PacmanCommand::ListInstalled => Ok(fixed(&["-Q"])),
    }
}

/// Whether a command may run without any package.
#[derive(Clone, Copy)]
enum Names {
    Required,
    Optional,
}

fn fixed(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

/// `operation`, then the fixed options, then (if there are names) `--` and the names.
fn build(
    operation: &[&str],
    options: &[&str],
    names: Vec<String>,
    needs: Names,
) -> Result<Vec<String>, ArgsRefused> {
    if names.is_empty() {
        return match needs {
            Names::Required => Err(ArgsRefused::NoTargets),
            Names::Optional => Ok([operation, options]
                .concat()
                .iter()
                .map(|p| p.to_string())
                .collect()),
        };
    }
    let mut built = fixed(operation);
    built.extend(fixed(options));
    built.push("--".to_string());
    built.extend(names);
    Ok(built)
}

/// Both parts must pass the allow-lists; the first bad one refuses everything (never "cleaned").
fn checked(packages: &[PackageRef]) -> Result<(), ArgsRefused> {
    let all_good = packages
        .iter()
        .all(|package| is_helper_repo_name(&package.repo) && is_valid_package_name(&package.name));
    if all_good {
        Ok(())
    } else {
        Err(ArgsRefused::BadName)
    }
}

/// `repo/name`, for `-S` (pins the repository the user confirmed).
fn repo_and_name(packages: &[PackageRef]) -> Result<Vec<String>, ArgsRefused> {
    checked(packages)?;
    Ok(packages
        .iter()
        .map(|package| format!("{}/{}", package.repo, package.name))
        .collect())
}

/// `name`, for `-D` (it takes installed package names).
fn name_only(packages: &[PackageRef]) -> Result<Vec<String>, ArgsRefused> {
    checked(packages)?;
    Ok(packages
        .iter()
        .map(|package| package.name.clone())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn two() -> Vec<PackageRef> {
        vec![pkg("extra", "dbeaver"), pkg("extra", "jre-openjdk")]
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// Every command the helper can build, with two packages (none for the two that may have none).
    fn all_commands() -> Vec<PacmanCommand> {
        vec![
            PacmanCommand::PendingUpdates,
            PacmanCommand::Sync,
            PacmanCommand::DryRunInstall(two()),
            PacmanCommand::DryRunUpdate(two()),
            PacmanCommand::DryRunUpdate(vec![]),
            PacmanCommand::RealInstall(two()),
            PacmanCommand::RealUpdate(two()),
            PacmanCommand::RealUpdate(vec![]),
            PacmanCommand::MarkAsDeps(two()),
            PacmanCommand::ListInstalled,
        ]
    }

    fn before_the_double_dash(args: &[String]) -> Vec<&str> {
        args.iter()
            .map(String::as_str)
            .take_while(|arg| *arg != "--")
            .collect()
    }

    // ---- the exact command lines ------------------------------------------------------------------------

    #[test]
    fn the_exact_argument_list_of_every_command() {
        let cases: Vec<(PacmanCommand, Vec<String>)> = vec![
            (PacmanCommand::PendingUpdates, strings(&["-Qu"])),
            (PacmanCommand::Sync, strings(&["-Sy"])),
            (
                PacmanCommand::DryRunInstall(two()),
                strings(&[
                    "-S",
                    "--print",
                    "--print-format",
                    "%r/%n %v",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            (
                PacmanCommand::DryRunUpdate(two()),
                strings(&[
                    "-Su",
                    "--print",
                    "--print-format",
                    "%r/%n %v",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            (
                PacmanCommand::DryRunUpdate(vec![]),
                strings(&["-Su", "--print", "--print-format", "%r/%n %v"]),
            ),
            (
                PacmanCommand::RealInstall(two()),
                strings(&[
                    "-S",
                    "--noconfirm",
                    "--ask",
                    "66",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            (
                PacmanCommand::RealUpdate(two()),
                strings(&[
                    "-Su",
                    "--noconfirm",
                    "--ask",
                    "66",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            (
                PacmanCommand::RealUpdate(vec![]),
                strings(&["-Su", "--noconfirm", "--ask", "66"]),
            ),
            (
                // -D and -Q take plain names, not repo/name.
                PacmanCommand::MarkAsDeps(two()),
                strings(&["-D", "--asdeps", "--", "dbeaver", "jre-openjdk"]),
            ),
            // The whole installed list: fixed argv, no names at all.
            (PacmanCommand::ListInstalled, strings(&["-Q"])),
        ];
        for (command, expected) in cases {
            assert_eq!(args(&command), Ok(expected), "{command:?}");
        }
    }

    #[test]
    fn the_program_is_pacman_by_absolute_path() {
        assert_eq!(PACMAN, "/usr/bin/pacman");
    }

    // ---- nothing the caller chose can sit before the double dash -----------------------------------------

    #[test]
    fn before_the_double_dash_there_is_only_fixed_text() {
        let fixed = [
            "-Qu",
            "-Sy",
            "-S",
            "-Su",
            "-Q",
            "-D",
            "--print",
            "--print-format",
            "%r/%n %v",
            "--noconfirm",
            "--ask",
            "66",
            "--asdeps",
        ];
        for command in all_commands() {
            let built = args(&command).unwrap();
            for arg in before_the_double_dash(&built) {
                assert!(
                    fixed.contains(&arg),
                    "{command:?}: {arg:?} is not fixed text"
                );
            }
        }
    }

    #[test]
    fn every_package_goes_after_the_double_dash() {
        for command in all_commands() {
            let built = args(&command).unwrap();
            let after: Vec<&String> = built
                .iter()
                .skip_while(|arg| *arg != "--")
                .skip(1)
                .collect();
            let names_given = !matches!(
                command,
                PacmanCommand::PendingUpdates
                    | PacmanCommand::ListInstalled
                    | PacmanCommand::Sync
                    | PacmanCommand::DryRunUpdate(_)
                    | PacmanCommand::RealUpdate(_)
            ) || built.contains(&"--".to_string());
            if names_given {
                assert!(built.contains(&"--".to_string()), "{command:?}");
                assert!(!after.is_empty(), "{command:?}");
            }
        }
    }

    // ---- the options that would make the helper a root shell (T-04, T-05) --------------------------------

    #[test]
    fn no_command_can_carry_a_dangerous_option() {
        let forbidden = [
            "--root",
            "-r",
            "--dbpath",
            "-b",
            "--config",
            "--hookdir",
            "--gpgdir",
            "--cachedir",
            "--sysroot",
            "--logfile",
            "--overwrite",
            "-d",
            "-dd",
            "--nodeps",
            "--assume-installed",
            "--noscriptlet",
            "--dbonly",
            "--disable-sandbox",
            "-U",
            "--upgrade",
            "-R",
            "--remove",
            "--needed",
            "--asexplicit",
            "--downloadonly",
            "-w",
            "--sysupgrade",
            "--refresh",
        ];
        for command in all_commands() {
            let built = args(&command).unwrap();
            for arg in &built {
                for option in forbidden {
                    assert!(
                        arg != option && !arg.starts_with(&format!("{option}=")),
                        "{command:?} contains {option}"
                    );
                }
            }
        }
    }

    #[test]
    fn y_is_only_in_the_sync_and_never_with_an_upgrade_or_a_install() {
        for command in all_commands() {
            let built = args(&command).unwrap();
            for arg in before_the_double_dash(&built) {
                let has_y = arg.starts_with('-') && !arg.starts_with("--") && arg.contains('y');
                if command == PacmanCommand::Sync {
                    assert_eq!(arg == "-Sy", has_y, "{arg}");
                } else {
                    assert!(!has_y, "{command:?}: {arg}");
                }
            }
        }
    }

    #[test]
    fn noconfirm_and_ask_belong_to_real_transactions_only() {
        for command in all_commands() {
            let built = args(&command).unwrap();
            let real = matches!(
                command,
                PacmanCommand::RealInstall(_) | PacmanCommand::RealUpdate(_)
            );
            assert_eq!(
                built.contains(&"--noconfirm".to_string()),
                real,
                "{command:?}"
            );
            assert_eq!(built.contains(&"--ask".to_string()), real, "{command:?}");
            if real {
                let at = built.iter().position(|arg| arg == "--ask").unwrap();
                assert_eq!(built.get(at + 1), Some(&"66".to_string()), "{command:?}");
            }
        }
    }

    #[test]
    fn print_belongs_to_the_dry_runs_only() {
        for command in all_commands() {
            let built = args(&command).unwrap();
            let dry = matches!(
                command,
                PacmanCommand::DryRunInstall(_) | PacmanCommand::DryRunUpdate(_)
            );
            assert_eq!(built.contains(&"--print".to_string()), dry, "{command:?}");
        }
    }

    // ---- hostile names are refused, never cleaned ---------------------------------------------------------

    fn commands_with(package: PackageRef) -> Vec<PacmanCommand> {
        vec![
            PacmanCommand::DryRunInstall(vec![package.clone()]),
            PacmanCommand::DryRunUpdate(vec![package.clone()]),
            PacmanCommand::RealInstall(vec![package.clone()]),
            PacmanCommand::RealUpdate(vec![package.clone()]),
            PacmanCommand::MarkAsDeps(vec![package.clone()]),
        ]
    }

    #[test]
    fn hostile_package_names_are_refused_in_every_command() {
        for name in [
            "",
            "-U",
            "--root=/x",
            "--force",
            ".hidden",
            "../x",
            "a b",
            "a\nb",
            "a\0b",
            "*",
            "/tmp/a.pkg.tar.zst",
            "file:///x",
            "caf\u{e9}",
            "\u{430}pp",
            "extra/vim",
        ] {
            for command in commands_with(pkg("extra", name)) {
                assert_eq!(
                    args(&command),
                    Err(ArgsRefused::BadName),
                    "{name:?} in {command:?}"
                );
            }
        }
    }

    #[test]
    fn hostile_repository_names_are_refused_in_every_command() {
        for repo in [
            "",
            "local",
            "ex/tra",
            "extra x",
            "--root=/x",
            "-U",
            "caf\u{e9}",
            "a\0b",
        ] {
            for command in commands_with(pkg(repo, "vim")) {
                assert_eq!(
                    args(&command),
                    Err(ArgsRefused::BadName),
                    "{repo:?} in {command:?}"
                );
            }
        }
    }

    #[test]
    fn one_bad_name_among_good_ones_refuses_the_whole_command() {
        let list = vec![
            pkg("extra", "vim"),
            pkg("extra", "--force"),
            pkg("extra", "git"),
        ];
        assert_eq!(
            args(&PacmanCommand::RealInstall(list)),
            Err(ArgsRefused::BadName)
        );
    }

    #[test]
    fn commands_that_need_packages_refuse_an_empty_list() {
        for command in [
            PacmanCommand::DryRunInstall(vec![]),
            PacmanCommand::RealInstall(vec![]),
            PacmanCommand::MarkAsDeps(vec![]),
        ] {
            assert_eq!(args(&command), Err(ArgsRefused::NoTargets), "{command:?}");
        }
    }

    #[test]
    fn the_refusal_never_repeats_the_name() {
        let refused = args(&PacmanCommand::RealInstall(vec![pkg(
            "extra",
            "SECRET/etc/shadow",
        )]));
        assert!(!format!("{refused:?}").contains("SECRET"));
    }

    #[test]
    fn the_order_of_the_packages_is_kept() {
        let list = vec![pkg("extra", "b"), pkg("core", "a"), pkg("extra", "c")];
        let built = args(&PacmanCommand::RealInstall(list)).unwrap();
        assert_eq!(
            &built[5..],
            strings(&["extra/b", "core/a", "extra/c"]).as_slice()
        );
    }
}
