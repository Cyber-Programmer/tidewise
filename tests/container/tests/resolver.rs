//! The install resolver (WBS 6.1) against the real pacman, with the dummy packages (`tests/container/pkgs`).
//!
//! The unit tests of `crates/pacman/src/resolve.rs` run on a model of pacman; these run `tidewise_pacman::resolve`
//! itself. Two things make them more than a repeat:
//!
//! - **The resolver runs as an unprivileged user.** The app never runs as root, so the test binary starts itself again
//!   as `nobody` (uid 65534) in "child mode" (`resolver_child`) and the child calls `resolve`. `pacman -S --print`
//!   needs no root and takes no lock, so this works; the parent, as root, does the set-up and the real install.
//! - **The plan is what the real run installs** (review 5.8 R-08): for each plan the resolver calls Ready, the real
//!   `pacman -S --noconfirm --ask 66` of the targets and then the providers installs exactly the packages of the plan.
//!
//! Real pacman, inside the Tidewise test container only (`tests/container/run.sh`).

use std::fs;
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use tidewise_pacman::{
    Blocker, PackageRef, ProviderAnswer, Question, Resolution, ResolveInput, resolve,
};

mod common;
use common::*;

/// Targets and provider answers (`dependency`, `package`) of one request.
type Case = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

const NOBODY: u32 = 65534;
const PREFIX: &str = "tidewise-dummy-";

fn dummy(name: &str) -> String {
    format!("{PREFIX}{name}")
}

fn pkg(name: &str) -> PackageRef {
    PackageRef {
        repo: REPO.to_string(),
        name: dummy(name),
    }
}

// ---- child mode: what runs as nobody ----------------------------------------------------------------------------

/// Not a test of its own: the parent starts it as `nobody` with the request in the environment
/// (`RESOLVER_TARGETS=a,b`, `RESOLVER_ANSWERS=dependency=pkg,...`, `RESOLVER_DBPATH=/folder/`). Without
/// `RESOLVER_TARGETS` it does nothing and passes, so a normal run of this file is not disturbed.
#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn resolver_child() {
    let Ok(targets) = std::env::var("RESOLVER_TARGETS") else {
        return;
    };
    let input = ResolveInput {
        targets: targets.split(',').map(pkg).collect(),
        provider_answers: std::env::var("RESOLVER_ANSWERS")
            .unwrap_or_default()
            .split(',')
            .filter(|pair| !pair.is_empty())
            .filter_map(|pair| pair.split_once('='))
            .map(|(dependency, chosen)| ProviderAnswer {
                dependency: dependency.to_string(),
                chosen: pkg(chosen),
            })
            .collect(),
        dbpath: std::env::var("RESOLVER_DBPATH").ok().map(PathBuf::from),
        ..ResolveInput::default()
    };
    let uid = fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next().map(str::to_string))
        .unwrap_or_default();
    println!("RESOLVED uid={uid} {}", summary(&resolve(&input)));
}

fn summary(result: &Result<Resolution, tidewise_pacman::PacmanError>) -> String {
    match result {
        Err(error) => format!("error: {error}"),
        Ok(Resolution::Blocked(blockers)) => blockers
            .iter()
            .map(|blocker| match blocker {
                Blocker::NothingProvides {
                    dependency,
                    required_by,
                } => format!("nothing-provides {dependency} for {required_by}"),
                other => format!("blocked {other:?}"),
            })
            .collect::<Vec<_>>()
            .join("; "),
        Ok(Resolution::NeedsAnswers(questions)) => questions
            .iter()
            .map(|question| match question {
                Question::ChooseProvider {
                    dependency,
                    candidates,
                    default,
                } => format!(
                    "ask {dependency} default={} [{}]",
                    candidates[*default].package.name,
                    candidates
                        .iter()
                        .map(|c| c.package.name.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                other => format!("question {other:?}"),
            })
            .collect::<Vec<_>>()
            .join("; "),
        Ok(Resolution::Ready(plan)) => format!(
            "ready items=[{}] providers=[{}]",
            plan.items
                .iter()
                .map(|item| format!("{}@{}:{:?}", item.package.name, item.version, item.role))
                .collect::<Vec<_>>()
                .join(","),
            plan.providers
                .iter()
                .map(|a| format!("{}={}", a.dependency, a.chosen.name))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

// ---- parent mode: root sets up, nobody resolves -----------------------------------------------------------------

/// Runs `resolve` as `nobody` and returns its summary line (without the uid).
fn resolved(targets: &[&str], answers: &[(&str, &str)], dbpath: Option<&str>) -> String {
    let answers: Vec<String> = answers
        .iter()
        .map(|(dependency, chosen)| format!("{dependency}={chosen}"))
        .collect();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "resolver_child", "--nocapture"])
        .args(["--test-threads=1"])
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("RESOLVER_TARGETS", targets.join(","))
        .env("RESOLVER_ANSWERS", answers.join(","))
        .uid(NOBODY)
        .gid(NOBODY)
        .stdin(Stdio::null());
    if let Some(folder) = dbpath {
        command.env("RESOLVER_DBPATH", folder);
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find_map(|line| line.split_once("RESOLVED uid=").map(|(_, rest)| rest))
        .unwrap_or_else(|| {
            panic!(
                "the child printed no result: {stdout}{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    let (uid, rest) = line.split_once(' ').unwrap();
    assert_eq!(uid, NOBODY.to_string(), "the resolver must not run as root");
    rest.to_string()
}

/// `name@version:Role` entries of a `ready items=[...]` summary.
fn plan_items(summary: &str) -> Vec<(String, String)> {
    let inner = summary
        .strip_prefix("ready items=[")
        .and_then(|rest| rest.split_once("] providers=["))
        .map(|(items, _)| items)
        .unwrap_or_else(|| panic!("not a ready plan: {summary}"));
    inner
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (name_version, _role) = entry.rsplit_once(':').unwrap();
            let (name, version) = name_version.split_once('@').unwrap();
            (name.to_string(), version.to_string())
        })
        .collect()
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_versioned_dependency_offers_the_providers_pacman_accepts_judged_with_the_other_targets() {
    assert_inside_the_test_container();
    reset_world();
    // vprov-1 provides tidewise-vvirtual=1, which does not satisfy >=2; the judge is pacman's own dry run.
    assert_eq!(
        resolved(&["needs-vvirtual", "hello"], &[], None),
        format!(
            "ask tidewise-vvirtual default={} [{},{}]",
            dummy("vprov-2"),
            dummy("vprov-2"),
            dummy("vprov-3")
        )
    );
    // A chosen provider that does not satisfy the dependency is dropped and the question comes back.
    assert_eq!(
        resolved(
            &["needs-vvirtual"],
            &[("tidewise-vvirtual", "vprov-1")],
            None
        ),
        format!(
            "ask tidewise-vvirtual default={} [{},{}]",
            dummy("vprov-2"),
            dummy("vprov-2"),
            dummy("vprov-3")
        )
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_provider_that_brings_another_provider_of_the_same_name_is_still_offered() {
    assert_inside_the_test_container();
    reset_world();
    assert_eq!(
        resolved(&["needs-chain"], &[], None),
        format!(
            "ask tidewise-chain default={} [{},{},{}]",
            dummy("chain-a"),
            dummy("chain-a"),
            dummy("chain-b"),
            dummy("chain-c")
        )
    );
    let ready = resolved(&["needs-chain"], &[("tidewise-chain", "chain-a")], None);
    assert_eq!(
        plan_items(&ready)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        [dummy("chain-b"), dummy("chain-a"), dummy("needs-chain")]
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_single_provider_is_named_and_nothing_that_satisfies_a_dependency_is_its_own_state() {
    assert_inside_the_test_container();
    reset_world();
    let single = resolved(&["needs-one"], &[], None);
    assert!(
        single.ends_with(&format!(
            "providers=[tidewise-single={}]",
            dummy("one-prov")
        )),
        "{single}"
    );
    assert_eq!(
        resolved(&["needs-stale"], &[], None),
        format!(
            "nothing-provides tidewise-stale>=2 for {}",
            dummy("needs-stale")
        )
    );
}

/// Threat model 42 (a): real pacman lists both providers of one name needed at two versions and asks nothing, and a
/// request can name only one of them, so the resolver stops here instead of calling it ready.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn one_name_needed_at_two_versions_is_blocked_before_it_reaches_the_helper() {
    assert_inside_the_test_container();
    reset_world();
    let listed = pacman_stdout(&[
        "-S",
        "--print",
        "--print-format",
        "%n",
        "--",
        "tidewise-dummy-needs-jv-8",
        "tidewise-dummy-needs-jv-9",
    ]);
    for provider in ["tidewise-dummy-jv-8", "tidewise-dummy-jv-9"] {
        assert!(listed.lines().any(|line| line == provider), "{listed}");
    }
    assert_eq!(
        resolved(&["needs-jv-8", "needs-jv-9"], &[], None),
        r#"blocked TwoVersionsNeeded { dependency: "tidewise-jv" }"#
    );
    // One of them alone stays an ordinary install.
    let ready = resolved(&["needs-jv-9"], &[], None);
    assert!(
        ready.ends_with(&format!("providers=[tidewise-jv={}]", dummy("jv-9"))),
        "{ready}"
    );
}

/// Review 5.8 R-08 with the resolver in the middle: whatever plan the resolver calls ready is what the real
/// transaction installs, package for package and version for version.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_plan_is_exactly_what_the_real_run_installs() {
    assert_inside_the_test_container();
    let cases: [Case; 5] = [
        (&["needs-one"], &[]),
        (&["needs-virtual"], &[("tidewise-virtual", "prov-b")]),
        (&["needs-chain"], &[("tidewise-chain", "chain-a")]),
        (
            &["needs-vvirtual", "hello"],
            &[("tidewise-vvirtual", "vprov-3")],
        ),
        (&["app"], &[]),
    ];
    for (targets, answers) in cases {
        reset_world();
        let ready = resolved(targets, answers, None);
        let mut planned = plan_items(&ready);

        // What the helper will run: the targets, then the chosen providers, all as repo/name.
        let mut names: Vec<String> = targets
            .iter()
            .map(|t| format!("{REPO}/{}", dummy(t)))
            .collect();
        names.extend(
            answers
                .iter()
                .map(|(_, chosen)| format!("{REPO}/{}", dummy(chosen))),
        );
        let mut args = vec!["-S", "--noconfirm", "--ask", "66", "--"];
        args.extend(names.iter().map(String::as_str));
        let real = pacman(&args);
        assert!(
            real.status.success(),
            "{targets:?}: {}",
            String::from_utf8_lossy(&real.stderr)
        );
        let mut installed: Vec<(String, String)> = pacman_stdout(&["-Q"])
            .lines()
            .filter(|line| line.starts_with(PREFIX))
            .filter_map(|line| line.split_once(' '))
            .map(|(name, version)| (name.to_string(), version.to_string()))
            .collect();
        planned.sort();
        installed.sort();
        assert_eq!(installed, planned, "{targets:?} {answers:?}: {ready}");
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_given_package_list_folder_is_the_one_that_is_read() {
    assert_inside_the_test_container();
    reset_world();
    // A private copy of the lists, the way WBS 6.11 will make one: the sync folder copied, `local` linked.
    let private = PathBuf::from("/tmp/lists-private");
    let _ = fs::remove_dir_all(&private);
    fs::create_dir_all(&private).unwrap();
    let copied = Command::new("/usr/bin/cp")
        .args(["-r", "/var/lib/pacman/sync"])
        .arg(&private)
        .status()
        .unwrap();
    assert!(copied.success());
    symlink("/var/lib/pacman/local", private.join("local")).unwrap();
    let chmod = Command::new("/usr/bin/chmod")
        .args(["-R", "a+rX"])
        .arg(&private)
        .status()
        .unwrap();
    assert!(chmod.success());

    // The real lists now get a newer tidewise-dummy-upd (2.0); the private copy keeps 1.0.
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let folder = "/tmp/lists-private/";
    let private_says = resolved(&["upd"], &[], Some(folder));
    let real_says = resolved(&["upd"], &[], None);
    let _ = fs::remove_dir_all(&private);
    assert!(
        private_says.contains(&format!("{}@1.0-1", dummy("upd"))),
        "{private_says}"
    );
    assert!(
        real_says.contains(&format!("{}@2.0-1", dummy("upd"))),
        "{real_says}"
    );
}

/// The app and the helper judge a provider pick separately (WBS 6.4a, golden rule 10): the helper must accept the
/// resolver's plan for each of these cases. Each plan is sent to the real helper, which installs it. Not every possible
/// plan is accepted: one name needed at two versions is refused (threat model, question 42 a).
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_helper_accepts_the_resolvers_plans_for_the_dummy_packages() {
    assert_inside_the_test_container();
    let cases: &[Case] = &[
        (&["needs-virtual"], &[("tidewise-virtual", "prov-a")]),
        (&["needs-virtual"], &[("tidewise-virtual", "prov-b")]),
        (&["needs-vvirtual"], &[("tidewise-vvirtual", "vprov-2")]),
        (
            &["needs-vvirtual", "hello"],
            &[("tidewise-vvirtual", "vprov-3")],
        ),
        (&["needs-one"], &[]),
        (&["needs-chain"], &[("tidewise-chain", "chain-a")]),
        (&["needs-chain"], &[("tidewise-chain", "chain-b")]),
        (&["needs-chain"], &[("tidewise-chain", "chain-c")]),
    ];
    for (targets, answers) in cases {
        reset_world();
        let ready = resolved(targets, answers, None);
        let items: Vec<Item> = plan_items(&ready)
            .into_iter()
            .map(|(name, version)| Item {
                repo: REPO.to_string(),
                name,
                version,
            })
            .collect();
        let providers: Vec<(String, String)> = ready
            .split_once("providers=[")
            .and_then(|(_, rest)| rest.strip_suffix(']'))
            .unwrap()
            .split(',')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (dependency, chosen) = pair.split_once('=').unwrap();
                (dependency.to_string(), chosen.to_string())
            })
            .collect();
        let names: Vec<String> = targets.iter().map(|name| dummy(name)).collect();
        let (request, summary) = install_request(
            &names
                .iter()
                .map(|name| (REPO, name.as_str()))
                .collect::<Vec<_>>(),
            &providers
                .iter()
                .map(|(dependency, chosen)| (dependency.as_str(), REPO, chosen.as_str()))
                .collect::<Vec<_>>(),
            &items,
        );
        let ran = run(&summary, request.as_bytes());
        assert_eq!(
            ran.lines().last(),
            Some(&"{\"finished\":\"ok\"}"),
            "{targets:?} {answers:?}: {ready}\n{}",
            ran.stdout
        );
        for item in &items {
            assert_eq!(
                installed_version(&item.name).as_deref(),
                Some(item.version.as_str()),
                "{targets:?} {answers:?}"
            );
        }
    }
}
