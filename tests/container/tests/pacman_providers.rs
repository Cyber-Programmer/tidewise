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
