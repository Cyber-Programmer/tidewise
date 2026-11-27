//! From the confirmed plan to what is sent to the helper (WBS 6.4b). The only place the app builds a request, so the
//! only place the summary for the password window is made: `validate` first, then `summary` and the bytes (threat
//! model 35 f: `summary` is total on any request, so nothing else stops a summary of a request never checked).
//! No I/O.

use tidewise_helper_protocol::{
    self as protocol, Action, Install, InvalidRequest, PROTOCOL_VERSION, ProtocolError,
    ProviderChoice, Request, TransactionItem, UpdateSystem, request_json, summary, validate,
};
use tidewise_pacman::{InstallDecision, InstallPlan, PackageRef, PlanItem, UpdatePlan};

/// What one helper run needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperRequest {
    /// The one argument: the text the password window shows.
    pub summary: String,
    /// The request, for the helper's stdin.
    pub json: Vec<u8>,
}

/// Why no request was made. Nothing was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    /// The plan needs a system update first; "update and install" is WBS 6.6.
    NeedsSystemUpdate,
    /// The helper's own rules refuse it: a bug in the app.
    Invalid(InvalidRequest),
    /// It could not be written, or is over the helper's size cap.
    Unwritable(ProtocolError),
}

/// The request for a plan the app may send today: a plain install on an up-to-date system.
pub fn install_request(plan: &InstallPlan) -> Result<HelperRequest, RequestError> {
    if !matches!(plan.decision, InstallDecision::Allowed { .. }) {
        return Err(RequestError::NeedsSystemUpdate);
    }
    sealed(Action::Install(Install {
        targets: plan.targets.iter().map(wire_ref).collect(),
        providers: plan
            .providers
            .iter()
            .map(|answer| ProviderChoice {
                dependency: answer.dependency.clone(),
                chosen: wire_ref(&answer.chosen),
            })
            .collect(),
        transaction: plan.items.iter().map(wire_item).collect(),
    }))
}

/// The request for a full system update: every line of the dry run, since the helper compares the whole list.
pub fn update_request(plan: &UpdatePlan) -> Result<HelperRequest, RequestError> {
    sealed(Action::UpdateSystem(UpdateSystem {
        transaction: plan.items.iter().map(wire_item).collect(),
    }))
}

/// `validate` first, then `summary` and the bytes: the only way a request is made.
fn sealed(action: Action) -> Result<HelperRequest, RequestError> {
    let request = Request {
        protocol_version: PROTOCOL_VERSION,
        action,
    };
    validate(&request).map_err(RequestError::Invalid)?;
    Ok(HelperRequest {
        summary: summary(&request),
        json: request_json(&request).map_err(RequestError::Unwritable)?,
    })
}

fn wire_item(item: &PlanItem) -> TransactionItem {
    TransactionItem {
        repo: item.package.repo.clone(),
        name: item.package.name.clone(),
        version: item.version.clone(),
    }
}

fn wire_ref(package: &PackageRef) -> protocol::PackageRef {
    protocol::PackageRef {
        repo: package.repo.clone(),
        name: package.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_helper_protocol::{Field, Reason, parse_request};
    use tidewise_pacman::{ProviderAnswer, Role};

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn item(name: &str, version: &str) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: version.to_string(),
            role: Role::Dependency,
            installed_version: None,
            download_size: 1,
            installed_size: 2,
            installed_size_text: String::new(),
            current_size: None,
        }
    }

    fn plan() -> InstallPlan {
        InstallPlan {
            decision: InstallDecision::Allowed { ignored: 0 },
            targets: vec![pkg("dbeaver")],
            providers: vec![ProviderAnswer {
                dependency: "java-runtime".to_string(),
                chosen: pkg("jre-openjdk"),
            }],
            items: vec![item("jre-openjdk", "25-1"), item("dbeaver", "25.2.1-1")],
            download_total: 2,
            installed_total: 4,
        }
    }

    #[test]
    fn a_plan_becomes_the_summary_and_the_request_the_helper_reads() {
        let made = install_request(&plan()).unwrap();
        assert_eq!(
            made.summary,
            "install extra/dbeaver with extra/jre-openjdk (2 packages)"
        );
        assert_eq!(
            String::from_utf8(made.json.clone()).unwrap(),
            r#"{"protocol_version":1,"action":{"install":{"targets":[{"repo":"extra","name":"dbeaver"}],"providers":[{"dependency":"java-runtime","chosen":{"repo":"extra","name":"jre-openjdk"}}],"transaction":[{"repo":"extra","name":"jre-openjdk","version":"25-1"},{"repo":"extra","name":"dbeaver","version":"25.2.1-1"}]}}}"#
        );
        // The helper accepts it, and works out the same summary.
        let read = parse_request(&made.json).unwrap();
        assert_eq!(tidewise_helper_protocol::summary(&read), made.summary);
    }

    #[test]
    fn a_plan_that_needs_a_system_update_makes_no_request() {
        let mut plan = plan();
        plan.decision = InstallDecision::MustUpdateSystem {
            count: 3,
            ignored: 0,
        };
        assert_eq!(install_request(&plan), Err(RequestError::NeedsSystemUpdate));
    }

    #[test]
    fn a_plan_the_helper_would_refuse_is_never_summarised() {
        let mut plan = plan();
        plan.targets.push(pkg("dbeaver"));
        assert_eq!(
            install_request(&plan),
            Err(RequestError::Invalid(InvalidRequest {
                field: Field::Targets,
                index: 1,
                reason: Reason::Duplicate,
            }))
        );
    }

    #[test]
    fn an_update_sends_the_whole_dry_run_as_the_transaction() {
        let plan = UpdatePlan {
            items: vec![item("zsh", "5.9-5"), item("new-pkg", "1-1")],
            ..UpdatePlan::default()
        };
        let made = update_request(&plan).unwrap();
        assert_eq!(made.summary, "update system (2 packages)");
        assert_eq!(
            String::from_utf8(made.json.clone()).unwrap(),
            r#"{"protocol_version":1,"action":{"update_system":{"transaction":[{"repo":"extra","name":"zsh","version":"5.9-5"},{"repo":"extra","name":"new-pkg","version":"1-1"}]}}}"#
        );
        let read = parse_request(&made.json).unwrap();
        assert_eq!(tidewise_helper_protocol::summary(&read), made.summary);
    }

    #[test]
    fn an_update_the_helper_would_refuse_is_never_summarised() {
        let empty = UpdatePlan::default();
        assert!(matches!(
            update_request(&empty),
            Err(RequestError::Invalid(_))
        ));
    }
}
