//! The request types: what the app asks the helper to do.
//!
//! Only what v0.1 needs: install, update the system, or both in one transaction. There is no field that
//! maps to a pacman option, no path, no URL and no free text: the helper chooses the pacman command line
//! from the variant alone (threat model T-04, T-05).

use serde::{Deserialize, Serialize};

/// The protocol version this build speaks. A request with any other number is refused.
pub const PROTOCOL_VERSION: u32 = 1;

/// One JSON object on the helper's stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub action: Action,
}

/// What to do. Written as `{"install": {...}}`, one key, so each variant is a plain struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// `pacman -S <targets and chosen providers>` against the current package lists.
    Install(Install),
    /// `pacman -Syu`.
    UpdateSystem(UpdateSystem),
    /// `pacman -Syu <targets and chosen providers>`.
    UpdateSystemAndInstall(Install),
}

/// An install: what the user asked for, which providers they chose, and the list they confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    /// The packages the user asked for.
    pub targets: Vec<PackageRef>,
    /// Answers to "choose a provider" questions (for example `java-runtime` -> `jre-openjdk`).
    pub providers: Vec<ProviderChoice>,
    /// The exact list the confirm screen showed. The helper redoes the dry run and refuses on any
    /// difference (TOCTOU, threat T-09).
    pub transaction: Vec<TransactionItem>,
}

/// A full system update and the list the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSystem {
    pub transaction: Vec<TransactionItem>,
}

/// A package in a repository: `extra/dbeaver`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub repo: String,
    pub name: String,
}

/// The user's answer to a provider question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderChoice {
    /// The name that several packages provide (`java-runtime`).
    pub dependency: String,
    /// The package the user picked for it.
    pub chosen: PackageRef,
}

/// One line of the confirmed list, as `pacman -S --print` reports it: repository, name, version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionItem {
    pub repo: String,
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    fn parse(text: &str) -> Result<Request, serde_json::Error> {
        serde_json::from_str(text)
    }

    #[test]
    fn each_fixture_parses_into_its_variant() {
        assert!(matches!(parse(INSTALL).unwrap().action, Action::Install(_)));
        assert!(matches!(
            parse(UPDATE).unwrap().action,
            Action::UpdateSystem(_)
        ));
        assert!(matches!(
            parse(BOTH).unwrap().action,
            Action::UpdateSystemAndInstall(_)
        ));
    }

    #[test]
    fn each_variant_survives_a_round_trip() {
        for text in [INSTALL, UPDATE, BOTH] {
            let request = parse(text).unwrap();
            let again = serde_json::to_string(&request).unwrap();
            assert_eq!(parse(&again).unwrap(), request);
        }
    }

    #[test]
    fn install_fixture_has_the_confirmed_list_and_the_provider_choice() {
        let Action::Install(install) = parse(INSTALL).unwrap().action else {
            panic!("not an install");
        };
        assert_eq!(install.targets.len(), 1);
        assert_eq!(install.providers.len(), 1);
        assert_eq!(install.transaction.len(), 5);
        assert_eq!(install.providers[0].dependency, "java-runtime");
    }

    #[test]
    fn an_unknown_field_is_refused_at_every_level() {
        let cases = [
            // top level
            r#"{"protocol_version":1,"root":"/x","action":{"update_system":{"transaction":[]}}}"#,
            // inside the action
            r#"{"protocol_version":1,"action":{"update_system":{"transaction":[],"--root":"/x"}}}"#,
            // inside a package reference
            r#"{"protocol_version":1,"action":{"install":{"targets":[{"repo":"extra","name":"a","path":"/x"}],"providers":[],"transaction":[]}}}"#,
            // inside a provider choice
            r#"{"protocol_version":1,"action":{"install":{"targets":[],"providers":[{"dependency":"a","chosen":{"repo":"extra","name":"b"},"force":true}],"transaction":[]}}}"#,
            // inside a transaction item
            r#"{"protocol_version":1,"action":{"update_system":{"transaction":[{"repo":"core","name":"a","version":"1-1","url":"http://x"}]}}}"#,
        ];
        for case in cases {
            assert!(parse(case).is_err(), "should refuse: {case}");
        }
    }

    #[test]
    fn an_unknown_action_is_refused() {
        for case in [
            r#"{"protocol_version":1,"action":{"remove":{"transaction":[]}}}"#,
            r#"{"protocol_version":1,"action":{"upgrade_file":{"path":"/tmp/a.pkg.tar.zst"}}}"#,
            r#"{"protocol_version":1,"action":"install"}"#,
            r#"{"protocol_version":1,"action":{}}"#,
        ] {
            assert!(parse(case).is_err(), "should refuse: {case}");
        }
    }

    #[test]
    fn an_action_with_two_keys_is_refused() {
        let case = r#"{"protocol_version":1,"action":{"update_system":{"transaction":[]},"install":{"targets":[],"providers":[],"transaction":[]}}}"#;
        assert!(parse(case).is_err());
    }

    #[test]
    fn a_repeated_key_is_refused() {
        let case = r#"{"protocol_version":1,"protocol_version":1,"action":{"update_system":{"transaction":[]}}}"#;
        assert!(parse(case).is_err());
    }

    #[test]
    fn a_missing_field_is_refused() {
        let case = r#"{"protocol_version":1}"#;
        assert!(parse(case).is_err());
    }
}
