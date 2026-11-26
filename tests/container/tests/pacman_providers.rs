//! What real pacman does with providers (WBS 6.1), as permanent tests.
//!
//! The install resolver (`crates/pacman/src/resolve.rs`) rests on facts about pacman that its unit tests only
//! *model*: that the "N providers available" prompt filters by version, that a dry run never asks and takes the first
//! provider, that naming a provider that does not fit makes pacman add another one, and what pacman prints when
//! nothing satisfies a dependency. The saved outputs in `tests/fixtures/pacman/container/` are what the unit tests
//! read. This file runs the same commands again in the container and compares, so a pacman that changes its words
//! fails here instead of silently making the fixtures (and the resolver's reading of them) stale (review 5.8 R-09,
//! the cheap first step). R-08 is in `pacman_prompts.rs`.
//!
//! Real pacman, as root, inside the Tidewise test container only (`tests/container/run.sh`).

use std::io::Write;
use std::process::{Command, Stdio};

mod common;
use common::*;

fn qualified(name: &str) -> String {
    format!("{REPO}/tidewise-dummy-{name}")
}

/// Runs pacman with `answers` written to its stdin (then stdin is closed), like a person typing.
fn pacman_typing(args: &[String], answers: &str) -> (String, String) {
    let mut child = Command::new("/usr/bin/pacman")
        .args(args)
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
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

macro_rules! fixture {
    ($stem:literal, $stream:literal) => {
        include_str!(concat!(
            "../../fixtures/pacman/container/",
            $stem,
            ".",
            $stream,
            ".txt"
        ))
    };
}

/// Runs the command and compares what it printed with the saved fixture(s).
fn same_as_fixture(stem: &str, args: &[String], answers: &str, stdout: &str, stderr: Option<&str>) {
    let (out, err) = pacman_typing(args, answers);
    assert_eq!(
        out, stdout,
        "stdout of {stem} ({args:?}) is not the fixture any more"
    );
    if let Some(expected) = stderr {
        assert_eq!(
            err, expected,
            "stderr of {stem} ({args:?}) is not the fixture any more"
        );
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

/// `pacman -S --print --print-format '%r/%n %v' -- <targets>` as the fixtures were captured.
fn print_args(targets: &[String]) -> Vec<String> {
    let mut args = strings(&["-S", "--print", "--print-format", "%r/%n %v", "--"]);
    args.extend_from_slice(targets);
    args
}

fn real_args(target: &str) -> Vec<String> {
    strings(&["-S", "--", target])
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_provider_fixtures_are_what_pacman_prints_now() {
    assert_inside_the_test_container();
    reset_world();

    // The prompt filters by version: for `>=2` it offers vprov-2 and vprov-3 and not vprov-1.
    same_as_fixture(
        "provider-prompt-versioned",
        &real_args(&qualified("needs-vvirtual")),
        "1\nn\n",
        fixture!("provider-prompt-versioned", "stdout"),
        Some(fixture!("provider-prompt-versioned", "stderr")),
    );
    // Without a version rule every provider is offered.
    same_as_fixture(
        "provider-prompt-unversioned",
        &real_args(&qualified("needs-virtual")),
        "1\nn\n",
        fixture!("provider-prompt-unversioned", "stdout"),
        None,
    );
    // One provider: no question at all.
    same_as_fixture(
        "provider-prompt-single",
        &real_args(&qualified("needs-one")),
        "n\n",
        fixture!("provider-prompt-single", "stdout"),
        Some(fixture!("provider-prompt-single", "stderr")),
    );
    // The dry run never asks: it takes the first matching provider.
    for (stem, target, expected) in [
        (
            "print-vvirtual-unnamed",
            "needs-vvirtual",
            fixture!("print-vvirtual-unnamed", "stdout"),
        ),
        (
            "print-one-unnamed",
            "needs-one",
            fixture!("print-one-unnamed", "stdout"),
        ),
        (
            "print-virtual-unnamed",
            "needs-virtual",
            fixture!("print-virtual-unnamed", "stdout"),
        ),
    ] {
        same_as_fixture(stem, &print_args(&[qualified(target)]), "", expected, None);
    }
    // EOF at the prompt: the default, then "Proceed?" reads EOF and pacman ends.
    same_as_fixture(
        "provider-prompt-eof",
        &real_args(&qualified("needs-vvirtual")),
        "",
        fixture!("provider-prompt-eof", "stdout"),
        Some(fixture!("provider-prompt-eof", "stderr")),
    );
    // The real run answers the same question with the default and installs what the dry run listed.
    same_as_fixture(
        "noconfirm-vvirtual-unnamed",
        &strings(&[
            "-S",
            "--noconfirm",
            "--ask",
            "66",
            "--",
            &qualified("needs-vvirtual"),
        ]),
        "",
        fixture!("noconfirm-vvirtual-unnamed", "stdout"),
        None,
    );

    // Option (b), with nothing installed: naming a provider that does not fit makes pacman add another one.
    reset_world();
    for (stem, candidate, expected) in [
        (
            "print-b-vprov-1-with-hello",
            "vprov-1",
            fixture!("print-b-vprov-1-with-hello", "stdout"),
        ),
        (
            "print-b-vprov-2-with-hello",
            "vprov-2",
            fixture!("print-b-vprov-2-with-hello", "stdout"),
        ),
        (
            "print-b-vprov-3-with-hello",
            "vprov-3",
            fixture!("print-b-vprov-3-with-hello", "stdout"),
        ),
    ] {
        let targets = [
            qualified("needs-vvirtual"),
            qualified("hello"),
            qualified(candidate),
        ];
        same_as_fixture(stem, &print_args(&targets), "", expected, None);
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_failure_and_chain_fixtures_are_what_pacman_prints_now() {
    assert_inside_the_test_container();
    reset_world();

    // Nothing satisfies the dependency (the only provider is too old): the same two lines, whatever else is named.
    let stale = qualified("needs-stale");
    same_as_fixture(
        "print-no-fit",
        &print_args(std::slice::from_ref(&stale)),
        "",
        fixture!("print-no-fit", "stdout"),
        Some(fixture!("print-no-fit", "stderr")),
    );
    same_as_fixture(
        "prompt-no-fit",
        &real_args(&stale),
        "n\n",
        fixture!("prompt-no-fit", "stdout"),
        Some(fixture!("prompt-no-fit", "stderr")),
    );
    same_as_fixture(
        "print-no-fit-old-provider-named",
        &print_args(&[stale, qualified("stale-prov")]),
        "",
        fixture!("print-no-fit-old-provider-named", "stdout"),
        Some(fixture!("print-no-fit-old-provider-named", "stderr")),
    );

    // chain-a provides tidewise-chain and needs chain-b, which provides it too.
    let needs = qualified("needs-chain");
    same_as_fixture(
        "provider-prompt-chain",
        &real_args(&needs),
        "1\nn\n",
        fixture!("provider-prompt-chain", "stdout"),
        None,
    );
    same_as_fixture(
        "print-chain-unnamed",
        &print_args(std::slice::from_ref(&needs)),
        "",
        fixture!("print-chain-unnamed", "stdout"),
        None,
    );
    for (stem, candidate, with_hello, alone) in [
        (
            "chain-a",
            "chain-a",
            fixture!("print-chain-a-with-hello", "stdout"),
            fixture!("print-chain-a-alone", "stdout"),
        ),
        (
            "chain-b",
            "chain-b",
            fixture!("print-chain-b-with-hello", "stdout"),
            fixture!("print-chain-b-alone", "stdout"),
        ),
        (
            "chain-c",
            "chain-c",
            fixture!("print-chain-c-with-hello", "stdout"),
            fixture!("print-chain-c-alone", "stdout"),
        ),
    ] {
        let with = [needs.clone(), qualified("hello"), qualified(candidate)];
        same_as_fixture(stem, &print_args(&with), "", with_hello, None);
        same_as_fixture(stem, &print_args(&[qualified(candidate)]), "", alone, None);
    }
}

/// The `-Si` fields the provider rule reads (WBS 6.4a). Build dates and sizes change with every image build, so the
/// records are compared field by field, not as text.
fn rule_fields(text: &str) -> Vec<String> {
    tidewise_pacman::parse_si(text)
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}/{} {} provides {:?} depends {:?}",
                p.repo, p.name, p.version, p.provides, p.depends_on
            )
        })
        .collect()
}

/// What the helper reads to find a provider pick (WBS 6.4a): `pacman -Si -- repo/name ...` of each dry-run list.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_details_fixtures_are_what_pacman_prints_now() {
    assert_inside_the_test_container();
    reset_world();

    macro_rules! si {
        ($stem:literal, $($name:literal),+) => {
            (
                $stem,
                vec![$(qualified($name)),+],
                include_str!(concat!("../../fixtures/pacman/container/", $stem, ".txt")),
            )
        };
    }
    for (stem, names, saved) in [
        si!("si-virtual-unnamed", "prov-a", "needs-virtual"),
        si!("si-vvirtual-unnamed", "vprov-2", "needs-vvirtual"),
        si!("si-one-unnamed", "one-prov", "needs-one"),
        si!("si-chain-unnamed", "chain-b", "chain-a", "needs-chain"),
        si!("si-virtual-prov-b-named", "prov-b", "needs-virtual"),
        si!(
            "si-b-vprov-1-with-hello",
            "vprov-2",
            "needs-vvirtual",
            "hello",
            "vprov-1"
        ),
    ] {
        let mut args = strings(&["-Si", "--"]);
        args.extend(names);
        let (out, _) = pacman_typing(&args, "");
        assert_eq!(
            rule_fields(&out),
            rule_fields(saved),
            "{stem} is not what pacman says any more"
        );
    }

    same_as_fixture(
        "print-virtual-prov-b-named",
        &print_args(&[qualified("needs-virtual"), qualified("prov-b")]),
        "",
        fixture!("print-virtual-prov-b-named", "stdout"),
        None,
    );
    same_as_fixture(
        "print-vprov-1-alone",
        &print_args(&[qualified("vprov-1")]),
        "",
        fixture!("print-vprov-1-alone", "stdout"),
        None,
    );
    // One unknown name: the others are still printed, and the exit is not 0.
    let (out, err) = pacman_typing(
        &strings(&["-Si", "--", &qualified("hello"), &qualified("nosuch")]),
        "",
    );
    assert_eq!(err, fixture!("si-one-not-found", "stderr"));
    assert_eq!(
        rule_fields(&out),
        rule_fields(fixture!("si-one-not-found", "stdout"))
    );
}

/// An installed package named in a dry run is listed again, as a reinstall (measured for WBS 6.4a): the helper's
/// "the chosen package is in the list" check does not turn such an answer away.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_installed_package_named_in_a_dry_run_is_listed_again() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-prov-b"]);
    same_as_fixture(
        "print-virtual-prov-b-named",
        &print_args(&[qualified("needs-virtual"), qualified("prov-b")]),
        "",
        fixture!("print-virtual-prov-b-named", "stdout"),
        None,
    );
}
