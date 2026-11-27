//! What real pacman does in a full `-Su` (WBS 6.5, decision 0009). The review screen and the helper's provider check
//! rest on these facts, so they are measured on every container run: what `-Qu` and `--print` list, and what the real
//! run does with a new virtual dependency, a replacement, a held (`IgnorePkg`) package and a conflict.
//!
//! The fixtures `tests/fixtures/pacman/container/update-*` are compared with what pacman prints now (the dry run
//! without its size column: sizes change when the packages are rebuilt).
//!
//! Real pacman, as root, inside the Tidewise test container only (`tests/container/run.sh`).

use std::io::Write;
use std::process::{Command, Output, Stdio};

mod common;
use common::*;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../fixtures/pacman/container/", $name))
    };
}

const UPD: &str = "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst";
const NEW: &str = "tidewise-dummy-new-1.0-1-any.pkg.tar.zst";
const UPD_VIRT: &str = "tidewise-dummy-upd-virt-2.0-1-any.pkg.tar.zst";
const NEEDS_UPD: &str = "tidewise-dummy-needs-upd-2.0-1-any.pkg.tar.zst";
const UPD_CONF: &str = "tidewise-dummy-upd-conf-2.0-1-any.pkg.tar.zst";
const UPD_SINGLE: &str = "tidewise-dummy-upd-single-2.0-1-any.pkg.tar.zst";
const UPD_DROP: &str = "tidewise-dummy-upd-drop-2.0-1-any.pkg.tar.zst";
const UPD_MOVING: &str = "tidewise-dummy-upd-moving-2.0-1-any.pkg.tar.zst";
const UPD_BOTH: &str = "tidewise-dummy-upd-both-2.0-1-any.pkg.tar.zst";
const OLD_NEWER: &str = "tidewise-dummy-old-2.0-1-any.pkg.tar.zst";
const NEW_DEP: &str = "tidewise-dummy-new-dep-1.0-1-any.pkg.tar.zst";

fn dry_run() -> Output {
    pacman(&["-Su", "--print", "--print-format", "%r/%n %v %s"])
}

fn real_run() -> Output {
    pacman(&["-Su", "--noconfirm", "--ask", "66"])
}

/// `repo/name version` of every line, without the size.
fn without_sizes(print: &str) -> Vec<String> {
    print
        .lines()
        .map(|line| line.rsplit_once(' ').unwrap().0.to_string())
        .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `pacman -Syu` with `answers` typed on its stdin: what someone at a terminal is asked. Not `--noconfirm`.
fn interactive_syu(answers: &str) -> Output {
    let mut child = Command::new("/usr/bin/pacman")
        .arg("-Syu")
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answers.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// `-Qu` and the dry run say exactly what the saved fixtures say.
fn matches_fixtures(qu: &str, print: &str) {
    assert_eq!(pacman_stdout(&["-Qu"]), qu);
    let now = dry_run();
    assert!(now.status.success(), "{}", text(&now.stderr));
    assert_eq!(without_sizes(&text(&now.stdout)), without_sizes(print));
}

fn setup(installed: &[&str], later: &[&str]) {
    assert_inside_the_test_container();
    reset_world();
    install_directly(installed);
    for file in later {
        add_newer(file);
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_upgrade_that_needs_a_virtual_name_makes_pacman_pick_a_provider_by_itself() {
    setup(
        &["tidewise-dummy-upd-virt", "tidewise-dummy-upd"],
        &[UPD_VIRT, UPD],
    );
    matches_fixtures(
        fixture!("update-provider-qu.stdout.txt"),
        fixture!("update-provider-print.stdout.txt"),
    );

    // The real run answers "2 providers available" with the default and installs it as a dependency.
    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(text(&real.stdout).contains("There are 2 providers available for tidewise-virtual"));
    assert_eq!(
        installed_version("tidewise-dummy-prov-a").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(
        install_reason("tidewise-dummy-prov-a"),
        "Installed as a dependency for another package"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_provider_that_is_installed_already_is_not_listed_again() {
    setup(
        &["tidewise-dummy-upd-virt", "tidewise-dummy-prov-b"],
        &[UPD_VIRT],
    );
    assert_eq!(
        without_sizes(&text(&dry_run().stdout)),
        ["tidewise-test/tidewise-dummy-upd-virt 2.0-1"]
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacement_is_in_the_dry_run_but_not_in_qu_and_the_real_run_declines_it() {
    setup(&["tidewise-dummy-old", "tidewise-dummy-upd"], &[NEW, UPD]);
    matches_fixtures(
        fixture!("update-replace-qu.stdout.txt"),
        fixture!("update-replace-print.stdout.txt"),
    );

    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert_eq!(installed_version("tidewise-dummy-new"), None);
    assert!(installed_version("tidewise-dummy-old").is_some());
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacement_alone_leaves_qu_empty_and_the_real_run_with_nothing_to_do() {
    setup(&["tidewise-dummy-old"], &[NEW]);
    assert_eq!(fixture!("update-replace-only-qu.stdout.txt"), "");
    let qu = pacman(&["-Qu"]);
    assert_eq!(qu.status.code(), Some(1));
    assert!(qu.stdout.is_empty());
    assert_eq!(
        without_sizes(&text(&dry_run().stdout)),
        without_sizes(fixture!("update-replace-only-print.stdout.txt"))
    );

    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(text(&real.stdout).contains("there is nothing to do"));
    assert_eq!(installed_version("tidewise-dummy-new"), None);
}

/// The review must not count what only a declined replacement needs (A1).
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn what_only_a_declined_replacement_needs_is_listed_but_not_installed() {
    setup(&["tidewise-dummy-old"], &[NEW_DEP]);
    assert_eq!(
        without_sizes(&text(&dry_run().stdout)),
        without_sizes(fixture!("update-replace-dep-print.stdout.txt"))
    );
    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert_eq!(installed_version("tidewise-dummy-new-dep"), None);
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
}

/// Review 6.5 R-01: the replaced package also has a newer version. The dry run lists only the replacer, whatever
/// `--ask` says; the real run declines the replacement and then upgrades the old package and what it needs. `-Qu`
/// is what shows it.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_declined_replacement_upgrades_the_old_package_although_no_dry_run_lists_it() {
    setup(&["tidewise-dummy-old"], &[OLD_NEWER, NEW]);
    matches_fixtures(
        fixture!("update-replace-newer-qu.stdout.txt"),
        fixture!("update-replace-newer-print.stdout.txt"),
    );
    let listed = text(&dry_run().stdout);
    for extra in [&["--ask", "66"][..], &["--noconfirm", "--ask", "66"]] {
        let mut args = vec!["-Su", "--print", "--print-format", "%r/%n %v %s"];
        args.extend(extra);
        assert_eq!(pacman_stdout(&args), listed, "{extra:?}");
    }

    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert_eq!(
        installed_version("tidewise-dummy-old").as_deref(),
        Some("2.0-1")
    );
    assert!(installed_version("tidewise-dummy-hello").is_some());
    assert_eq!(installed_version("tidewise-dummy-new"), None);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_held_package_is_marked_by_qu_and_left_out_of_the_dry_run() {
    setup(
        &[
            "tidewise-dummy-upd",
            "tidewise-dummy-upd-virt",
            "tidewise-dummy-prov-a",
        ],
        &[UPD, UPD_VIRT],
    );
    ignore_package("tidewise-dummy-upd");
    matches_fixtures(
        fixture!("update-held-qu.stdout.txt"),
        fixture!("update-held-print.stdout.txt"),
    );

    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(text(&real.stderr).contains("ignoring package upgrade"));
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_upgrade_that_needs_a_held_package_fails_the_dry_run_and_the_whole_update() {
    setup(
        &["tidewise-dummy-upd", "tidewise-dummy-needs-upd"],
        &[UPD, NEEDS_UPD],
    );
    ignore_package("tidewise-dummy-upd");
    assert_eq!(
        pacman_stdout(&["-Qu"]),
        fixture!("update-needs-held-qu.stdout.txt")
    );

    let dry = dry_run();
    assert_eq!(dry.status.code(), Some(1));
    assert_eq!(
        text(&dry.stdout),
        fixture!("update-needs-held-print.stdout.txt")
    );
    assert_eq!(
        text(&dry.stderr),
        fixture!("update-needs-held-print.stderr.txt")
    );

    // "skip the above package?" defaults to no: nothing at all is upgraded.
    let real = real_run();
    assert!(!real.status.success());
    assert_eq!(
        installed_version("tidewise-dummy-needs-upd").as_deref(),
        Some("1.0-1")
    );
}

/// The advice on the "held package blocks the update" card: pacman really does offer to skip (card 9, B1).
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_interactive_syu_offers_to_skip_the_package_and_updates_the_rest() {
    setup(
        &[
            "tidewise-dummy-upd",
            "tidewise-dummy-needs-upd",
            "tidewise-dummy-upd-conf",
        ],
        &[UPD, NEEDS_UPD, UPD_CONF],
    );
    ignore_package("tidewise-dummy-upd");

    // Enter takes the default, which is no: nothing is upgraded, not even the unrelated package.
    let refused = interactive_syu("\n\n");
    assert!(!refused.status.success());
    assert_eq!(
        text(&refused.stderr),
        fixture!("update-needs-held-syu.stderr.txt")
    );
    assert!(
        text(&refused.stdout).contains(
            ":: The following package cannot be upgraded due to unresolvable dependencies:"
        )
    );
    assert_eq!(
        installed_version("tidewise-dummy-upd-conf").as_deref(),
        Some("1.0-1")
    );

    // "y" skips it, "y" proceeds: the rest is upgraded, that one and the held one stay.
    let skipped = interactive_syu("y\ny\n");
    assert!(skipped.status.success(), "{}", text(&skipped.stderr));
    for (name, version) in [
        ("tidewise-dummy-upd-conf", "2.0-1"),
        ("tidewise-dummy-needs-upd", "1.0-1"),
        ("tidewise-dummy-upd", "1.0-1"),
    ] {
        assert_eq!(installed_version(name).as_deref(), Some(version), "{name}");
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_conflict_in_an_update_is_not_in_the_dry_run_and_aborts_everything() {
    setup(
        &[
            "tidewise-dummy-upd-conf",
            "tidewise-dummy-conf-x",
            "tidewise-dummy-upd",
        ],
        &[UPD_CONF, UPD],
    );
    let dry = dry_run();
    assert!(dry.status.success());
    assert!(text(&dry.stdout).contains("tidewise-dummy-upd-conf 2.0-1"));

    let real = real_run();
    assert!(!real.status.success());
    assert!(text(&real.stderr).contains("unresolvable package conflicts detected"));
    // Not even the unrelated upgrade happens.
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("1.0-1")
    );
    assert!(installed_version("tidewise-dummy-conf-x").is_some());
}

/// The update rule's lone provider (decision 0009): one package can provide the name, so nothing is asked.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_name_with_a_single_provider_is_not_asked_about() {
    setup(&["tidewise-dummy-upd-single"], &[UPD_SINGLE]);
    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(!text(&real.stdout).contains("providers available"));
    assert!(installed_version("tidewise-dummy-one-prov").is_some());
}

/// An installed package provides the name: pacman asks nothing, although another listed package provides it too.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_name_an_installed_package_provides_is_not_asked_about() {
    setup(
        &["tidewise-dummy-upd-both", "tidewise-dummy-moving-b"],
        &[UPD_BOTH],
    );
    assert_eq!(
        without_sizes(&text(&dry_run().stdout)),
        [
            "tidewise-test/tidewise-dummy-moving-a 1.0-1",
            "tidewise-test/tidewise-dummy-upd-both 2.0-1",
        ]
    );
    // `-T` says it is satisfied by what is installed.
    assert!(pacman(&["-T", "--", "tidewise-moving"]).status.success());
    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(!text(&real.stdout).contains("providers available"));
}

/// Why `-T` alone is not enough: the installed provider's new version drops the name in the same update, and pacman
/// asks although `-T` says it is satisfied now.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_installed_provider_that_drops_the_name_leaves_a_choice_although_deptest_is_satisfied() {
    setup(
        &["tidewise-dummy-upd-drop", "tidewise-dummy-upd-moving"],
        &[UPD_DROP, UPD_MOVING],
    );
    assert!(pacman(&["-T", "--", "tidewise-moving"]).status.success());
    let real = real_run();
    assert!(real.status.success(), "{}", text(&real.stderr));
    assert!(text(&real.stdout).contains("providers available for tidewise-moving"));
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn deptest_answers_for_installed_packages_and_respects_versions() {
    setup(&["tidewise-dummy-moving-b"], &[]);
    for (dependency, code) in [
        ("tidewise-moving", 0),
        ("tidewise-moving>=2", 0),
        ("tidewise-moving>=3", 127),
        ("tidewise-dummy-moving-b=1.0-1", 0),
        ("tidewise-virtual", 127),
    ] {
        let out = pacman(&["-T", "--", dependency]);
        assert_eq!(out.status.code(), Some(code), "{dependency}");
        let expected = if code == 0 {
            String::new()
        } else {
            format!("{dependency}\n")
        };
        assert_eq!(text(&out.stdout), expected, "{dependency}");
    }
}

/// The helper counts providers with `-Ss '^name$'` on its private copy: packages that only PROVIDE the name must be
/// found (the condition of the human's approval).
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn ss_on_a_private_copy_finds_packages_that_only_provide_the_name() {
    assert_inside_the_test_container();
    reset_world();
    private_folder("/run/tidewise-measure/", 0o711);
    let db = [
        "--dbpath",
        "/run/tidewise-measure/",
        "--logfile",
        "/dev/null",
    ];
    let mut sync = vec!["-Sy"];
    sync.extend(db);
    assert!(pacman(&sync).status.success());
    let search = |pattern: &str| -> (Option<i32>, Vec<String>) {
        let out = pacman(&["-Ss", "--dbpath", "/run/tidewise-measure/", "--", pattern]);
        let names = text(&out.stdout)
            .lines()
            .filter(|line| !line.starts_with(' '))
            .map(|line| line.split(' ').next().unwrap().to_string())
            .collect();
        (out.status.code(), names)
    };
    assert_eq!(
        search("^tidewise-virtual$"),
        (
            Some(0),
            vec![
                "tidewise-test/tidewise-dummy-prov-a".to_string(),
                "tidewise-test/tidewise-dummy-prov-b".to_string(),
            ]
        )
    );
    // The fixture the unit tests read is what pacman prints.
    assert_eq!(
        pacman_stdout(&[
            "-Ss",
            "--dbpath",
            "/run/tidewise-measure/",
            "--",
            "^tidewise-virtual$"
        ]),
        fixture!("update-provider-ss.stdout.txt")
    );
    // A versioned `Provides` is found by its name too.
    assert_eq!(search("^tidewise-vvirtual$").1.len(), 3);
    assert_eq!(
        search("^tidewise-single$"),
        (
            Some(0),
            vec!["tidewise-test/tidewise-dummy-one-prov".to_string()]
        )
    );
    assert_eq!(search("^tidewise-nothing$"), (Some(1), vec![]));
}

/// Why a soname is judged at all (review 6.5 R-02): pacman asks between two providers of the same version, and does
/// not offer the one of another version.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn pacman_asks_between_two_providers_of_one_shared_library_version() {
    assert_inside_the_test_container();
    reset_world();
    let out = pacman(&["-S", "--noconfirm", "--", "tidewise-dummy-needs-so"]);
    let text = text(&out.stdout);
    assert!(
        text.contains("2 providers available for libtidewise.so=1-64"),
        "{text}"
    );
    assert!(!text.contains("tidewise-dummy-so-32"), "{text}");
}

/// The helper counts providers with ONE search for many names: it must find what one search per name finds, a name
/// that is only provided and one with plus signs included.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn one_batched_search_finds_what_one_search_per_name_finds() {
    assert_inside_the_test_container();
    reset_world();
    let found = |pattern: &str| -> Vec<String> {
        text(&pacman(&["-Ss", "--", pattern]).stdout)
            .lines()
            .filter(|line| !line.starts_with(' '))
            .map(|line| line.split(' ').next().unwrap().to_string())
            .collect()
    };
    let mut one_by_one: Vec<String> = [
        r"^libtide\+\+\.so$",
        r"^libtidewise\.so$",
        "^tidewise-single$",
        "^tidewise-virtual$",
        "^tidewise-dummy-hello$",
    ]
    .iter()
    .flat_map(|pattern| found(pattern))
    .collect();
    one_by_one.sort();
    let batch = r"^(libtide\+\+\.so|libtidewise\.so|tidewise-single|tidewise-virtual|tidewise-dummy-hello)$";
    let mut batched = found(batch);
    batched.sort();
    assert_eq!(batched, one_by_one);
    assert!(batched.contains(&format!("{REPO}/tidewise-dummy-one-prov")));
    assert_eq!(
        pacman_stdout(&["-Ss", "--", batch]),
        fixture!("search-batch.stdout.txt")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn si_reads_a_private_copy_of_the_lists() {
    assert_inside_the_test_container();
    reset_world();
    private_folder("/run/tidewise-measure/", 0o711);
    assert!(
        pacman(&[
            "-Sy",
            "--dbpath",
            "/run/tidewise-measure/",
            "--logfile",
            "/dev/null"
        ])
        .status
        .success()
    );
    let si = pacman(&[
        "-Si",
        "--dbpath",
        "/run/tidewise-measure/",
        "--",
        "tidewise-test/tidewise-dummy-prov-a",
    ]);
    assert!(si.status.success(), "{}", text(&si.stderr));
    assert!(text(&si.stdout).contains("Provides        : tidewise-virtual"));
}
