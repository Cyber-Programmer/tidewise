//! Validation: the allow-list a request must pass. Pure, no I/O.
//!
//! We refuse, we never clean: the first thing wrong makes the whole request fail, and the error says which
//! list and which position, never the offending text (threat model T-06, T-15).

use serde::{Deserialize, Serialize};

use crate::Request;

/// Most packages the user can ask for in one request.
pub const MAX_TARGETS: usize = 100;
/// Most provider answers in one request.
pub const MAX_PROVIDERS: usize = 100;
/// Most lines in the confirmed list. A long-delayed full update can be a couple of thousand packages.
pub const MAX_TRANSACTION: usize = 5_000;
/// Longest package or dependency name, in bytes.
pub const MAX_NAME_BYTES: usize = 128;
/// Longest version text, in bytes.
pub const MAX_VERSION_BYTES: usize = 64;

/// Which list of the request is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Targets,
    Providers,
    Transaction,
}

/// What is wrong with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The list is empty but must have something in it.
    Empty,
    /// The list is longer than its limit.
    TooMany,
    /// Not a legal package (or dependency) name, or longer than `MAX_NAME_BYTES`.
    BadPackageName,
    /// Not a repository name the helper accepts.
    BadRepoName,
    /// Not a legal version text, or longer than `MAX_VERSION_BYTES`.
    BadVersion,
    /// The same package (or dependency) appears twice.
    Duplicate,
    /// A target or a chosen provider is missing from the confirmed list (or is there under another repository).
    NotInTransaction,
}

/// The first problem found: where it is, never what was written there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvalidRequest {
    pub field: Field,
    /// Position in that list, counting from 0 (0 for whole-list problems).
    pub index: usize,
    pub reason: Reason,
}

/// Checks a parsed request against the allow-list. `Ok` means every check passed.
///
/// STUB (red commit): accepts everything until the checks are written.
pub fn validate(_request: &Request) -> Result<(), InvalidRequest> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Install, PROTOCOL_VERSION, PackageRef, ProviderChoice, TransactionItem,
        UpdateSystem,
    };

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn item(repo: &str, name: &str, version: &str) -> TransactionItem {
        TransactionItem {
            repo: repo.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn choice(dependency: &str, repo: &str, name: &str) -> ProviderChoice {
        ProviderChoice {
            dependency: dependency.to_string(),
            chosen: pkg(repo, name),
        }
    }

    fn request(action: Action) -> Request {
        Request {
            protocol_version: PROTOCOL_VERSION,
            action,
        }
    }

    /// A good install of `extra/dbeaver` with provider `java-runtime` -> `extra/jre-openjdk`.
    fn good_install() -> Install {
        Install {
            targets: vec![pkg("extra", "dbeaver")],
            providers: vec![choice("java-runtime", "extra", "jre-openjdk")],
            transaction: vec![
                item("extra", "jre-openjdk", "25.0.1.u8-1"),
                item("extra", "dbeaver", "25.2.1-1"),
            ],
        }
    }

    fn good_update() -> UpdateSystem {
        UpdateSystem {
            transaction: vec![item("core", "glibc", "2.42-2")],
        }
    }

    fn err(request: &Request) -> InvalidRequest {
        validate(request).expect_err("should be refused")
    }

    fn bad(field: Field, index: usize, reason: Reason) -> InvalidRequest {
        InvalidRequest {
            field,
            index,
            reason,
        }
    }

    /// Names the hostile-input table of ENGINEERING.md section 2 puts in front of the helper.
    fn hostile_names() -> Vec<String> {
        let mut names: Vec<String> = [
            "",
            "--force",
            "-dd",
            "--root=/x",
            "--",
            "-x",
            ".x",
            "../x",
            "a/b",
            "*",
            "/tmp/a.pkg.tar.zst",
            "file:///x",
            "http://x",
            "a\0b",
            "a b",
            "a;b",
            "$(x)",
            "a\n",
            "\u{1b}[31mx",
            "\u{202e}x",
            "\u{430}pp",
            "\u{ff41}\u{ff50}\u{ff50}",
            "caf\u{e9}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        names.push("a".repeat(MAX_NAME_BYTES + 1));
        names.push("a".repeat(1_000_000));
        names
    }

    #[test]
    fn the_sample_requests_are_valid() {
        for text in [INSTALL, UPDATE, BOTH] {
            let request: Request = serde_json::from_str(text).unwrap();
            assert_eq!(validate(&request), Ok(()));
        }
    }

    #[test]
    fn a_good_request_of_each_kind_is_valid() {
        assert_eq!(validate(&request(Action::Install(good_install()))), Ok(()));
        assert_eq!(
            validate(&request(Action::UpdateSystemAndInstall(good_install()))),
            Ok(())
        );
        assert_eq!(
            validate(&request(Action::UpdateSystem(good_update()))),
            Ok(())
        );
    }

    #[test]
    fn empty_lists_are_refused() {
        let mut no_targets = good_install();
        no_targets.targets.clear();
        assert_eq!(
            err(&request(Action::Install(no_targets.clone()))),
            bad(Field::Targets, 0, Reason::Empty)
        );
        assert_eq!(
            err(&request(Action::UpdateSystemAndInstall(no_targets))),
            bad(Field::Targets, 0, Reason::Empty)
        );

        let mut no_transaction = good_install();
        no_transaction.transaction.clear();
        assert_eq!(
            err(&request(Action::Install(no_transaction))),
            bad(Field::Transaction, 0, Reason::Empty)
        );
        assert_eq!(
            err(&request(Action::UpdateSystem(UpdateSystem {
                transaction: vec![]
            }))),
            bad(Field::Transaction, 0, Reason::Empty)
        );
    }

    #[test]
    fn providers_may_be_empty() {
        let mut install = good_install();
        install.providers.clear();
        install.transaction = vec![item("extra", "dbeaver", "25.2.1-1")];
        assert_eq!(validate(&request(Action::Install(install))), Ok(()));
    }

    #[test]
    fn too_many_of_anything_is_refused() {
        let mut install = good_install();
        install.targets = (0..=MAX_TARGETS)
            .map(|n| pkg("extra", &format!("p{n}")))
            .collect();
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 0, Reason::TooMany)
        );

        let mut install = good_install();
        install.providers = (0..=MAX_PROVIDERS)
            .map(|n| choice(&format!("d{n}"), "extra", &format!("p{n}")))
            .collect();
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Providers, 0, Reason::TooMany)
        );

        let items: Vec<TransactionItem> = (0..=MAX_TRANSACTION)
            .map(|n| item("extra", &format!("p{n}"), "1-1"))
            .collect();
        assert_eq!(
            err(&request(Action::UpdateSystem(UpdateSystem {
                transaction: items
            }))),
            bad(Field::Transaction, 0, Reason::TooMany)
        );
    }

    #[test]
    fn hostile_package_names_are_refused_everywhere_a_name_can_go() {
        for name in hostile_names() {
            let mut install = good_install();
            install.targets[0].name = name.clone();
            assert_eq!(
                err(&request(Action::Install(install))),
                bad(Field::Targets, 0, Reason::BadPackageName),
                "target {name:?}"
            );

            let mut install = good_install();
            install.providers[0].dependency = name.clone();
            assert_eq!(
                err(&request(Action::Install(install))),
                bad(Field::Providers, 0, Reason::BadPackageName),
                "dependency {name:?}"
            );

            let mut install = good_install();
            install.providers[0].chosen.name = name.clone();
            assert_eq!(
                err(&request(Action::Install(install))),
                bad(Field::Providers, 0, Reason::BadPackageName),
                "chosen {name:?}"
            );

            let mut update = good_update();
            update.transaction[0].name = name.clone();
            assert_eq!(
                err(&request(Action::UpdateSystem(update))),
                bad(Field::Transaction, 0, Reason::BadPackageName),
                "transaction {name:?}"
            );
        }
    }

    #[test]
    fn hostile_repo_names_are_refused_everywhere_a_repo_can_go() {
        let mut repos = hostile_names();
        repos.push("local".to_string());
        repos.push("\u{dc}n\u{ef}".to_string());
        for repo in repos {
            let mut install = good_install();
            install.targets[0].repo = repo.clone();
            assert_eq!(
                err(&request(Action::Install(install))),
                bad(Field::Targets, 0, Reason::BadRepoName),
                "target repo {repo:?}"
            );

            let mut install = good_install();
            install.providers[0].chosen.repo = repo.clone();
            assert_eq!(
                err(&request(Action::Install(install))),
                bad(Field::Providers, 0, Reason::BadRepoName),
                "chosen repo {repo:?}"
            );

            let mut update = good_update();
            update.transaction[0].repo = repo.clone();
            assert_eq!(
                err(&request(Action::UpdateSystem(update))),
                bad(Field::Transaction, 0, Reason::BadRepoName),
                "transaction repo {repo:?}"
            );
        }
    }

    #[test]
    fn bad_versions_are_refused() {
        let long = "1".repeat(MAX_VERSION_BYTES + 1);
        for version in [
            "", "-1", "1 2", "1;2", "1/2", "$(x)", "1\n", "a\0", "\u{ff11}", "1\u{430}", &long,
        ] {
            let mut update = good_update();
            update.transaction[0].version = version.to_string();
            assert_eq!(
                err(&request(Action::UpdateSystem(update))),
                bad(Field::Transaction, 0, Reason::BadVersion),
                "version {version:?}"
            );
        }
    }

    #[test]
    fn real_looking_versions_are_accepted() {
        for version in [
            "1.0-1",
            "1:2.0-1",
            "25.0.1.u8-1",
            "2.42+r17+gabc-1",
            "6.0~rc1-1",
            "5_1-2",
        ] {
            let mut update = good_update();
            update.transaction[0].version = version.to_string();
            assert_eq!(
                validate(&request(Action::UpdateSystem(update))),
                Ok(()),
                "version {version:?}"
            );
        }
    }

    #[test]
    fn duplicates_are_refused() {
        // the same target twice
        let mut install = good_install();
        install.targets.push(pkg("extra", "dbeaver"));
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 1, Reason::Duplicate)
        );

        // the same name in two repositories
        let mut install = good_install();
        install.targets.push(pkg("multilib", "dbeaver"));
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 1, Reason::Duplicate)
        );

        // the same dependency answered twice
        let mut install = good_install();
        install
            .providers
            .push(choice("java-runtime", "extra", "jre17-openjdk"));
        install
            .transaction
            .push(item("extra", "jre17-openjdk", "17.0.1-1"));
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Providers, 1, Reason::Duplicate)
        );

        // the same package chosen for two dependencies
        let mut install = good_install();
        install
            .providers
            .push(choice("java-environment", "extra", "jre-openjdk"));
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Providers, 1, Reason::Duplicate)
        );

        // a chosen provider that is also a target
        let mut install = good_install();
        install.providers[0].chosen = pkg("extra", "dbeaver");
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Providers, 0, Reason::Duplicate)
        );

        // the same package twice in the confirmed list
        let mut install = good_install();
        install
            .transaction
            .push(item("extra", "dbeaver", "25.2.1-1"));
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Transaction, 2, Reason::Duplicate)
        );
    }

    #[test]
    fn targets_and_choices_must_be_in_the_confirmed_list_from_the_same_repo() {
        // target missing
        let mut install = good_install();
        install.transaction.retain(|i| i.name != "dbeaver");
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 0, Reason::NotInTransaction)
        );

        // target there, but from another repository
        let mut install = good_install();
        install.targets[0].repo = "multilib".to_string();
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 0, Reason::NotInTransaction)
        );

        // chosen provider missing
        let mut install = good_install();
        install.transaction.retain(|i| i.name != "jre-openjdk");
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Providers, 0, Reason::NotInTransaction)
        );
    }

    #[test]
    fn the_error_names_the_position_of_the_first_problem() {
        let mut install = good_install();
        install.targets = vec![
            pkg("extra", "dbeaver"),
            pkg("extra", "gimp"),
            pkg("extra", "--force"),
        ];
        assert_eq!(
            err(&request(Action::Install(install))),
            bad(Field::Targets, 2, Reason::BadPackageName)
        );
    }
}
