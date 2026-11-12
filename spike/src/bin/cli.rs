//! Prototype A: run `pacman -Ss` / `-Si` and parse the text. Throwaway.
use std::process::{Command, Stdio};
use tidewise_spike::{PACKAGES, SEARCH_TERM, detail_line, parse_args, search_line};

/// Absolute program, argument list, empty environment except LC_ALL=C, no stdin, no shell.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start {program}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "pacman failed ({:?}): {}",
            out.status.code(),
            err.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `repo/name version [group] [installed]` then one indented description line.
fn parse_ss(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut head: Option<(String, String, bool)> = None;
    for line in text.lines() {
        if line.starts_with(' ') {
            if let Some((repo_name, version, installed)) = head.take() {
                let (repo, name) = repo_name.split_once('/').unwrap_or(("", &repo_name));
                lines.push(search_line(repo, name, &version, installed, line.trim()));
            }
        } else {
            let mut words = line.split_whitespace();
            let repo_name = words.next().unwrap_or("").to_string();
            let version = words.next().unwrap_or("").to_string();
            head = Some((repo_name, version, line.contains("[installed")));
        }
    }
    lines
}

/// "None" means an empty list; otherwise names are separated by whitespace.
fn words(value: &str) -> Vec<String> {
    if value == "None" {
        return Vec::new();
    }
    value.split_whitespace().map(str::to_string).collect()
}

/// Blocks of `Key : value`, separated by a blank line. Continuation lines start with a space.
fn parse_si(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for block in text.split("\n\n") {
        let (mut name, mut repo, mut version) = (String::new(), String::new(), String::new());
        let (mut download, mut size) = (String::new(), String::new());
        let (mut depends, mut provides) = (Vec::new(), Vec::new());
        for line in block.lines() {
            if line.starts_with(' ') {
                continue;
            }
            let Some((key, value)) = line.split_once(" : ") else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "Repository" => repo = value.to_string(),
                "Name" => name = value.to_string(),
                "Version" => version = value.to_string(),
                "Download Size" => download = value.to_string(),
                "Installed Size" => size = value.to_string(),
                "Depends On" => depends = words(value),
                "Provides" => provides = words(value),
                _ => {}
            }
        }
        if !name.is_empty() {
            lines.push(detail_line(
                &name, &repo, &version, &download, &size, &depends, &provides,
            ));
        }
    }
    lines
}

fn main() {
    let args = parse_args();
    let ss = ["--dbpath", &args.dbpath, "-Ss", SEARCH_TERM];
    let mut si = vec!["--dbpath", &args.dbpath, "-Si"];
    si.extend(PACKAGES);
    let result = run(&args.pacman, &ss).and_then(|a| run(&args.pacman, &si).map(|b| (a, b)));
    match result {
        Ok((search, details)) => {
            let mut all = parse_ss(&search);
            all.extend(parse_si(&details));
            all.sort();
            println!("{}", all.join("\n"));
        }
        Err(message) => {
            eprintln!("A failed: {message}");
            std::process::exit(1);
        }
    }
}
