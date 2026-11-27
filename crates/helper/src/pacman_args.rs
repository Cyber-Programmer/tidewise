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
//! `-y` appears only in `PrivateSync`, and there only with `--dbpath /run/tidewise-helper-lists/ --logfile /dev/null`: no
//! command can refresh the real package lists (decision 0007, golden rule 4). The private copy is moved into place by
//! `lists.rs`, only after its dry run equals the list the user confirmed.

use tidewise_helper_protocol::PackageRef;
use tidewise_pacman::{Dependency, dependency_text};

use crate::lists::LISTS_DBPATH;
use tidewise_names::{is_helper_repo_name, is_valid_package_name};

/// pacman, by absolute path (no `PATH` lookup, no shell).
pub const PACMAN: &str = "/usr/bin/pacman";

/// What the helper may ask pacman to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacmanCommand {
    /// `pacman -Qu`: pending updates (a read).
    PendingUpdates,
    /// `pacman -Sy --dbpath /run/tidewise-helper-lists/ --logfile /dev/null`: refresh the PRIVATE copy of the package
    /// lists. Once per update, never before a plain install. The real lists are never refreshed by any command.
    PrivateSync,
    /// `pacman -Su --print ... --dbpath /run/tidewise-helper-lists/ --logfile /dev/null [-- ...]`: what a full update
    /// (plus these, if any) would do against the private copy (a read). Must equal the confirmed list before the
    /// private lists are moved into place.
    PrivateDryRunUpdate(Vec<PackageRef>),
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
    /// `pacman -Si -- repo/name ...`: what the repositories say about these (a read). The provider check of an install
    /// reads `Depends On` and `Provides` from it (golden rule 10).
    PackageDetails(Vec<PackageRef>),
    /// `pacman -Qu --dbpath /run/tidewise-helper-lists/`: what the private copy says has a newer version (a read). An
    /// update whose dry run leaves one of them out is refused (review 6.5 R-01).
    PrivatePendingUpdates,
    /// `pacman -Si --dbpath /run/tidewise-helper-lists/ -- repo/name ...`: the same, from the PRIVATE copy of the
    /// package lists (a read). The provider check of a system update, before anything is moved (golden rule 10).
    PrivatePackageDetails(Vec<PackageRef>),
    /// `pacman -Ss --dbpath /run/tidewise-helper-lists/ -- ^(name|name)$`: which packages of the private copy are
    /// named so or provide one of them (a read). The update's provider check counts them (decision 0009).
    PrivateSearch(Vec<String>),
    /// `pacman -Ss -- ^(name|name)$`: the same against the real lists, for the provider check of an install.
    Search(Vec<String>),
    /// `pacman -T -- name[<op>version]`: is this satisfied by what is installed? (a read; exit 127 means no).
    DepTest(Dependency),
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
        PacmanCommand::PrivateSync => Ok(fixed(&[
            "-Sy",
            "--dbpath",
            LISTS_DBPATH,
            "--logfile",
            "/dev/null",
        ])),
        PacmanCommand::PrivateDryRunUpdate(packages) => build(
            &["-Su"],
            &[
                &dry_run[..],
                &["--dbpath", LISTS_DBPATH, "--logfile", "/dev/null"],
            ]
            .concat(),
            repo_and_name(packages)?,
            Names::Optional,
        ),
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
        PacmanCommand::PackageDetails(packages) => {
            build(&["-Si"], &[], repo_and_name(packages)?, Names::Required)
        }
        PacmanCommand::PrivatePendingUpdates => Ok(fixed(&["-Qu", "--dbpath", LISTS_DBPATH])),
        PacmanCommand::PrivatePackageDetails(packages) => build(
            &["-Si"],
            &["--dbpath", LISTS_DBPATH],
            repo_and_name(packages)?,
            Names::Required,
        ),
        PacmanCommand::PrivateSearch(_) | PacmanCommand::Search(_) => Err(ArgsRefused::NoTargets),
        PacmanCommand::DepTest(dependency) => {
            let text = dependency_text(dependency).ok_or(ArgsRefused::BadName)?;
            build(&["-T"], &[], vec![text], Names::Required)
        }
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
    use tidewise_pacman::{Comparison, VersionRule};

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
            PacmanCommand::PrivateSync,
            PacmanCommand::PrivatePendingUpdates,
            PacmanCommand::PrivateDryRunUpdate(two()),
            PacmanCommand::PrivateDryRunUpdate(vec![]),
            PacmanCommand::DryRunInstall(two()),
            PacmanCommand::DryRunUpdate(two()),
            PacmanCommand::DryRunUpdate(vec![]),
            PacmanCommand::RealInstall(two()),
            PacmanCommand::RealUpdate(two()),
            PacmanCommand::RealUpdate(vec![]),
            PacmanCommand::MarkAsDeps(two()),
            PacmanCommand::PackageDetails(two()),
            PacmanCommand::PrivatePackageDetails(two()),
            PacmanCommand::PrivateSearch(strings(&["tidewise-virtual"])),
            PacmanCommand::Search(strings(&["libstdc++.so", "a.b"])),
            PacmanCommand::DepTest(need("java-runtime", Some("17"))),
            PacmanCommand::ListInstalled,
        ]
    }

    /// A number for each kind of command. The match has no `_` arm on purpose (review 5.8 R-07): a new `PacmanCommand`
    /// does not compile here until it is given a number, and `LAST` must grow with it, which makes
    /// `every_command_kind_is_in_the_table` fail until `all_commands()` holds one of the new kind. Every template test
    /// (no dangerous option, `-y` only in the sync, `--noconfirm` only in real runs, `--print` only in dry runs) loops
    /// over that table, so a command outside it would be checked by none of them.
    fn kind(command: &PacmanCommand) -> usize {
        match command {
            PacmanCommand::PendingUpdates => 0,
            PacmanCommand::PrivateSync => 1,
            PacmanCommand::DryRunInstall(_) => 2,
            PacmanCommand::DryRunUpdate(_) => 3,
            PacmanCommand::RealInstall(_) => 4,
            PacmanCommand::RealUpdate(_) => 5,
            PacmanCommand::MarkAsDeps(_) => 6,
            PacmanCommand::ListInstalled => 7,
            PacmanCommand::PrivateDryRunUpdate(_) => 8,
            PacmanCommand::PackageDetails(_) => 9,
            PacmanCommand::PrivatePackageDetails(_) => 10,
            PacmanCommand::PrivateSearch(_) => 11,
            PacmanCommand::DepTest(_) => 12,
            PacmanCommand::PrivatePendingUpdates => 13,
            PacmanCommand::Search(_) => 14,
        }
    }
    /// The highest number `kind` hands out. Raise it together with a new arm.
    const LAST: usize = 14;

    #[test]
    fn every_command_kind_is_in_the_table() {
        let in_the_table: std::collections::BTreeSet<usize> =
            all_commands().iter().map(kind).collect();
        let every_kind: std::collections::BTreeSet<usize> = (0..=LAST).collect();
        assert_eq!(
            in_the_table, every_kind,
            "all_commands() lacks a kind of command"
        );
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
            (
                PacmanCommand::PrivateSync,
                strings(&[
                    "-Sy",
                    "--dbpath",
                    "/run/tidewise-helper-lists/",
                    "--logfile",
                    "/dev/null",
                ]),
            ),
            (
                PacmanCommand::PrivateDryRunUpdate(two()),
                strings(&[
                    "-Su",
                    "--print",
                    "--print-format",
                    "%r/%n %v",
                    "--dbpath",
                    "/run/tidewise-helper-lists/",
                    "--logfile",
                    "/dev/null",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            (
                PacmanCommand::PrivateDryRunUpdate(vec![]),
                strings(&[
                    "-Su",
                    "--print",
                    "--print-format",
                    "%r/%n %v",
                    "--dbpath",
                    "/run/tidewise-helper-lists/",
                    "--logfile",
                    "/dev/null",
                ]),
            ),
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
            (
                PacmanCommand::PackageDetails(two()),
                strings(&["-Si", "--", "extra/dbeaver", "extra/jre-openjdk"]),
            ),
            (
                PacmanCommand::PrivatePendingUpdates,
                strings(&["-Qu", "--dbpath", "/run/tidewise-helper-lists/"]),
            ),
            // The provider check of a system update: the private copy, its fixed path, nothing else.
            (
                PacmanCommand::PrivatePackageDetails(two()),
                strings(&[
                    "-Si",
                    "--dbpath",
                    "/run/tidewise-helper-lists/",
                    "--",
                    "extra/dbeaver",
                    "extra/jre-openjdk",
                ]),
            ),
            // Every name anchored and escaped, so it matches itself only: `.` and `+` are not left to the regex.
            (
                PacmanCommand::PrivateSearch(strings(&["gtk+-x11.so"])),
                strings(&[
                    "-Ss",
                    "--dbpath",
                    "/run/tidewise-helper-lists/",
                    "--",
                    "^(gtk\\+-x11\\.so)$",
                ]),
            ),
            (
                PacmanCommand::Search(strings(&["libstdc++.so", "a.b", "libGL.so"])),
                strings(&["-Ss", "--", "^(libstdc\\+\\+\\.so|a\\.b|libGL\\.so)$"]),
            ),
            (
                PacmanCommand::DepTest(need("java-runtime", Some("1:17"))),
                strings(&["-T", "--", "java-runtime>=1:17"]),
            ),
            (
                PacmanCommand::DepTest(need("sh", None)),
                strings(&["-T", "--", "sh"]),
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
            "-Si",
            "-Ss",
            "-T",
            "-Q",
            "-D",
            "--print",
            "--print-format",
            "%r/%n %v",
            "--noconfirm",
            "--ask",
            "66",
            "--asdeps",
            // The two private commands only, and only these exact values (checked in
            // `the_two_dangerous_looking_options_exist_only_with_their_fixed_values`).
            "--dbpath",
            "/run/tidewise-helper-lists/",
            "--logfile",
            "/dev/null",
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
                    | PacmanCommand::PrivateSync
                    | PacmanCommand::PrivatePendingUpdates
                    | PacmanCommand::DryRunUpdate(_)
                    | PacmanCommand::PrivateDryRunUpdate(_)
                    | PacmanCommand::RealUpdate(_)
            ) || built.contains(&"--".to_string());
            if names_given {
                assert!(built.contains(&"--".to_string()), "{command:?}");
                assert!(!after.is_empty(), "{command:?}");
            }
            // Reads that take no names carry no `--` at all (R-05): `-Q` must stay a fixed argv.
            if matches!(command, PacmanCommand::ListInstalled) {
                assert!(!built.contains(&"--".to_string()), "{command:?}");
            }
        }
    }

    /// `name`, or `name>=version`.
    fn need(name: &str, at_least: Option<&str>) -> Dependency {
        Dependency {
            name: name.to_string(),
            rule: at_least.map(|version| VersionRule {
                comparison: Comparison::GreaterOrEqual,
                version: version.to_string(),
            }),
        }
    }

    #[test]
    fn the_search_and_the_deptest_take_nothing_that_is_not_plain() {
        // Nothing outside the allow-list can reach the pattern, wherever in the list it is: no `|`, `)`, `*` or space
        // that would turn one name into a wider regex.
        for bad in [
            "-x", "a b", "", "a$(b)", "a|b", "*", "../x", "a)|(.*", "a\\", ".a", "a\n",
        ] {
            for names in [
                strings(&[bad]),
                strings(&["good", bad]),
                strings(&[bad, "good"]),
            ] {
                for command in [
                    PacmanCommand::PrivateSearch(names.clone()),
                    PacmanCommand::Search(names.clone()),
                ] {
                    assert_eq!(args(&command), Err(ArgsRefused::BadName), "{command:?}");
                }
            }
            assert_eq!(
                args(&PacmanCommand::DepTest(need(bad, None))),
                Err(ArgsRefused::BadName),
                "{bad:?}"
            );
        }
        assert_eq!(
            args(&PacmanCommand::Search(vec![])),
            Err(ArgsRefused::NoTargets)
        );
        for version in ["", "-1", "1 2", "1;2"] {
            assert_eq!(
                args(&PacmanCommand::DepTest(need("a", Some(version)))),
                Err(ArgsRefused::BadName),
                "{version:?}"
            );
        }
    }

    /// The commands that read or write the private copy of the lists.
    fn may_name_a_dbpath(command: &PacmanCommand) -> bool {
        may_name_a_logfile(command)
            || matches!(
                command,
                PacmanCommand::PrivatePackageDetails(_)
                    | PacmanCommand::PrivateSearch(_)
                    | PacmanCommand::PrivatePendingUpdates
            )
    }

    /// The two that would otherwise write to pacman's log (`-Si` writes nothing there).
    fn may_name_a_logfile(command: &PacmanCommand) -> bool {
        matches!(
            command,
            PacmanCommand::PrivateSync | PacmanCommand::PrivateDryRunUpdate(_)
        )
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
                    // The private commands carry these, with fixed values (next test).
                    if (option == "--dbpath" && may_name_a_dbpath(&command))
                        || (option == "--logfile" && may_name_a_logfile(&command))
                    {
                        continue;
                    }
                    assert!(
                        arg != option && !arg.starts_with(&format!("{option}=")),
                        "{command:?} contains {option}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_two_dangerous_looking_options_exist_only_with_their_fixed_values() {
        for command in all_commands() {
            let built = args(&command).unwrap();
            for (option, value) in [
                ("--dbpath", "/run/tidewise-helper-lists/"),
                ("--logfile", "/dev/null"),
            ] {
                let at: Vec<usize> = built
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| *arg == option || arg.starts_with(&format!("{option}=")))
                    .map(|(i, _)| i)
                    .collect();
                let allowed = if option == "--dbpath" {
                    may_name_a_dbpath(&command)
                } else {
                    may_name_a_logfile(&command)
                };
                if allowed {
                    assert_eq!(at.len(), 1, "{command:?}: {option}");
                    assert_eq!(built.get(at[0]), Some(&option.to_string()));
                    assert_eq!(
                        built.get(at[0] + 1),
                        Some(&value.to_string()),
                        "{command:?}"
                    );
                } else {
                    assert!(at.is_empty(), "{command:?} carries {option}");
                }
            }
        }
    }

    #[test]
    fn y_only_ever_comes_with_the_private_dbpath_and_a_null_logfile() {
        // Decision 0007: no command can refresh the real package lists. Any command with a -y carries
        // `--dbpath /run/tidewise-helper-lists/` and `--logfile /dev/null`, both as exact adjacent pairs.
        let mut refreshes = 0;
        for command in all_commands() {
            let built = args(&command).unwrap();
            let has_y = before_the_double_dash(&built)
                .iter()
                .any(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.contains('y'));
            if !has_y {
                continue;
            }
            refreshes += 1;
            assert_eq!(command, PacmanCommand::PrivateSync);
            assert!(
                built
                    .windows(2)
                    .any(|p| p[0] == "--dbpath" && p[1] == "/run/tidewise-helper-lists/"),
                "{command:?} refreshes without the private --dbpath: {built:?}"
            );
            assert!(
                built
                    .windows(2)
                    .any(|p| p[0] == "--logfile" && p[1] == "/dev/null"),
                "{command:?} refreshes without --logfile /dev/null: {built:?}"
            );
        }
        assert_eq!(refreshes, 1, "exactly one command refreshes the lists");
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
                PacmanCommand::DryRunInstall(_)
                    | PacmanCommand::DryRunUpdate(_)
                    | PacmanCommand::PrivateDryRunUpdate(_)
            );
            assert_eq!(built.contains(&"--print".to_string()), dry, "{command:?}");
        }
    }

    // ---- hostile names are refused, never cleaned ---------------------------------------------------------

    fn commands_with(package: PackageRef) -> Vec<PacmanCommand> {
        vec![
            PacmanCommand::DryRunInstall(vec![package.clone()]),
            PacmanCommand::DryRunUpdate(vec![package.clone()]),
            PacmanCommand::PrivateDryRunUpdate(vec![package.clone()]),
            PacmanCommand::RealInstall(vec![package.clone()]),
            PacmanCommand::RealUpdate(vec![package.clone()]),
            PacmanCommand::MarkAsDeps(vec![package.clone()]),
            PacmanCommand::PackageDetails(vec![package.clone()]),
            PacmanCommand::PrivatePackageDetails(vec![package.clone()]),
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
            PacmanCommand::PackageDetails(vec![]),
            PacmanCommand::PrivatePackageDetails(vec![]),
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
