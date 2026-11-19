//! What real pacman does with providers (WBS 6.1, Step 0 measurement; becomes permanent tests as the resolver is built).
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
fn pacman_typing(args: &[&str], answers: &str) -> (i32, String, String) {
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
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn show(title: &str, args: &[&str], answers: &str) {
    let (code, out, err) = pacman_typing(args, answers);
    println!(
        "@@@ {title}\n@@@ args: {args:?} stdin: {answers:?}\n@@@ exit {code}\n@@@ stdout\n{out}@@@ stderr\n{err}@@@ end"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn measure_provider_prompts() {
    assert_inside_the_test_container();
    reset_world();
    let versioned = qualified("needs-vvirtual");
    let single = qualified("needs-one");
    let plain = qualified("needs-virtual");

    // 1. The prompt for a versioned dependency (>=2): which providers does it list? Answer 1, then no at "Proceed?".
    show(
        "prompt, tidewise-vvirtual>=2",
        &["-S", "--", &versioned],
        "1\nn\n",
    );
    // The same without a version: all providers should be listed.
    show(
        "prompt, tidewise-virtual (no version)",
        &["-S", "--", &plain],
        "1\nn\n",
    );
    // 2. A single provider: does pacman ask at all?
    show("prompt, single provider", &["-S", "--", &single], "n\n");
    // 3. What --print picks.
    for target in [&versioned, &single, &plain] {
        show(
            "dry run, unnamed",
            &["-S", "--print", "--print-format", "%r/%n %v", "--", target],
            "",
        );
    }
    // 4. EOF on stdin at the prompt.
    show(
        "prompt, stdin closed at once",
        &["-S", "--", &versioned],
        "",
    );
    // 5. --noconfirm without --print: which one is taken (not installed: only the dry run of it is kept, then reset).
    show(
        "noconfirm, unnamed, versioned",
        &["-S", "--noconfirm", "--ask", "66", "--", &versioned],
        "",
    );
    // Step 5 installed packages. Nothing may be installed for the option (b) cases: an installed provider that fits
    // would answer the dependency by itself and there would be no provider question.
    reset_world();
    println!(
        "@@@ installed before option (b): {:?}",
        pacman_stdout(&["-Qq"])
            .lines()
            .filter(|l| l.contains("tidewise-dummy"))
            .collect::<Vec<_>>()
    );
    // 6. Option (b), nothing installed that fits: one dry run per candidate, with the user's other target (hello) in
    // the same request. A candidate that does not satisfy `>=2` must make pacman add another provider.
    for candidate in ["vprov-1", "vprov-2", "vprov-3"] {
        show(
            &format!("option (b): needs-vvirtual + hello + {candidate}"),
            &[
                "-S",
                "--print",
                "--print-format",
                "%r/%n %v",
                "--",
                &versioned,
                &qualified("hello"),
                &qualified(candidate),
            ],
            "",
        );
    }
    // 7. Why did naming vprov-1 add nothing? Other orders, no other target, and a real run that is answered "n".
    let (v, p1) = (qualified("needs-vvirtual"), qualified("vprov-1"));
    fn with<'a>(targets: &[&'a str]) -> Vec<&'a str> {
        let mut args = vec!["-S", "--print", "--print-format", "%r/%n %v", "--"];
        args.extend_from_slice(targets);
        args
    }
    show(
        "probe: vprov-1 first, then needs-vvirtual",
        &with(&[&p1, &v]),
        "",
    );
    show(
        "probe: needs-vvirtual + vprov-1 only",
        &with(&[&v, &p1]),
        "",
    );
    show(
        "probe: real run, needs-vvirtual + vprov-1, answer n",
        &["-S", "--", &v, &p1],
        "n\n",
    );
    show("probe: only vprov-1 (provider of =1)", &with(&[&p1]), "");
    println!(
        "@@@ installed after: {}",
        pacman_stdout(&["-Q"])
            .lines()
            .filter(|l| l.contains("tidewise-dummy"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
