//! The hostile-input table of ENGINEERING.md section 2, at the type level: every row is a full request as raw
//! bytes, sent through `parse_request` (the one door), with the exact refusal we expect (threat model T-01,
//! T-04 to T-07). Nothing here needs root or pacman.

use serde_json::{Value, json};
use tidewise_helper_protocol::{
    Field, InvalidRequest, MAX_REQUEST_BYTES, MAX_TARGETS, MAX_TRANSACTION, ProtocolError, Reason,
    parse_request,
};

fn target(name: &str) -> Value {
    json!({"repo": "extra", "name": name})
}

fn line(name: &str, version: &str) -> Value {
    json!({"repo": "extra", "name": name, "version": version})
}

/// A good install of `name` with a confirmed list that contains it.
fn install_of(name: &str) -> Vec<u8> {
    json!({"protocol_version": 1, "action": {"install": {
        "targets": [target(name)],
        "providers": [],
        "transaction": [line(name, "1-1")]
    }}})
    .to_string()
    .into_bytes()
}

fn invalid(field: Field, index: usize, reason: Reason) -> ProtocolError {
    ProtocolError::Invalid(InvalidRequest {
        field,
        index,
        reason,
    })
}

#[test]
fn the_good_request_the_table_starts_from_is_accepted() {
    assert!(parse_request(&install_of("dbeaver")).is_ok());
}

/// Every hostile package name of the table, with its label: what the protocol must refuse as a name.
fn hostile_names() -> Vec<(&'static str, String)> {
    let long = "a".repeat(1_000_000);
    let names: Vec<(&str, &str)> = vec![
        ("force flag", "--force"),
        ("nodeps flag", "-dd"),
        ("root option", "--root=/x"),
        ("double dash", "--"),
        ("single dash", "-"),
        ("leading dash", "-x"),
        ("leading dot", ".x"),
        ("parent directory", "../x"),
        ("absolute path", "/etc/passwd"),
        ("star", "*"),
        ("local package file", "/tmp/a.pkg.tar.zst"),
        ("file url", "file:///x"),
        ("http url", "http://x"),
        ("NUL", "a\u{0}b"),
        ("newline", "a\nb"),
        ("space", "a b"),
        ("semicolon", "a;b"),
        ("command substitution", "$(x)"),
        ("backtick", "`x`"),
        ("escape sequence", "\u{1b}[31mx"),
        ("right-to-left override", "\u{202e}x"),
        ("Cyrillic look-alike", "\u{430}pp"),
        ("full-width letters", "\u{ff41}\u{ff50}\u{ff50}"),
        ("accent", "caf\u{e9}"),
        ("empty", ""),
        ("one million bytes", long.as_str()),
    ];
    names
        .into_iter()
        .map(|(label, name)| (label, name.to_string()))
        .collect()
}

#[test]
fn hostile_names_are_refused_as_names() {
    for (label, name) in hostile_names() {
        assert_eq!(
            parse_request(&install_of(&name)),
            Err(invalid(Field::Targets, 0, Reason::BadPackageName)),
            "{label}"
        );
    }
}

#[test]
fn hostile_repository_names_are_refused() {
    for (label, repo) in [
        ("local database", "local"),
        ("slash", "a/b"),
        ("leading dash", "-x"),
        ("option", "--root=/x"),
        ("space", "my repo"),
        ("accent", "\u{dc}n\u{ef}"),
        ("Cyrillic look-alike", "extr\u{430}"),
        ("empty", ""),
    ] {
        let request = json!({"protocol_version": 1, "action": {"install": {
            "targets": [{"repo": repo, "name": "dbeaver"}],
            "providers": [],
            "transaction": [{"repo": repo, "name": "dbeaver", "version": "1-1"}]
        }}})
        .to_string();
        assert_eq!(
            parse_request(request.as_bytes()),
            Err(invalid(Field::Targets, 0, Reason::BadRepoName)),
            "{label}"
        );
    }
}

#[test]
fn hostile_versions_are_refused() {
    for (label, version) in [
        ("option", "--force"),
        ("leading dash", "-1"),
        ("semicolon", "1;rm"),
        ("slash", "1/2"),
        ("space", "1 2"),
        ("NUL", "1\u{0}"),
        ("full-width digit", "\u{ff11}"),
        ("empty", ""),
    ] {
        let request = json!({"protocol_version": 1, "action": {"update_system": {
            "transaction": [line("glibc", version)]
        }}})
        .to_string();
        assert_eq!(
            parse_request(request.as_bytes()),
            Err(invalid(Field::Transaction, 0, Reason::BadVersion)),
            "{label}"
        );
    }
}

#[test]
fn empty_and_oversize_lists_are_refused() {
    let empty_targets = json!({"protocol_version": 1, "action": {"install": {
        "targets": [], "providers": [], "transaction": [line("a", "1-1")]
    }}});
    assert_eq!(
        parse_request(empty_targets.to_string().as_bytes()),
        Err(invalid(Field::Targets, 0, Reason::Empty))
    );

    let empty_update =
        json!({"protocol_version": 1, "action": {"update_system": {"transaction": []}}});
    assert_eq!(
        parse_request(empty_update.to_string().as_bytes()),
        Err(invalid(Field::Transaction, 0, Reason::Empty))
    );

    let many: Vec<Value> = (0..10_000).map(|n| target(&format!("p{n}"))).collect();
    assert!(many.len() > MAX_TARGETS);
    let ten_thousand_targets = json!({"protocol_version": 1, "action": {"install": {
        "targets": many, "providers": [], "transaction": [line("p0", "1-1")]
    }}});
    assert_eq!(
        parse_request(ten_thousand_targets.to_string().as_bytes()),
        Err(invalid(Field::Targets, 0, Reason::TooMany))
    );

    let lines: Vec<Value> = (0..=MAX_TRANSACTION)
        .map(|n| line(&format!("p{n}"), "1-1"))
        .collect();
    let too_long_update =
        json!({"protocol_version": 1, "action": {"update_system": {"transaction": lines}}});
    assert_eq!(
        parse_request(too_long_update.to_string().as_bytes()),
        Err(invalid(Field::Transaction, 0, Reason::TooMany))
    );
}

#[test]
fn flags_paths_and_urls_have_no_field_to_live_in() {
    for extra in [
        r#""noconfirm":true"#,
        r#""overwrite":"*""#,
        r#""nodeps":true"#,
        r#""root":"/x""#,
        r#""config":"/tmp/evil.conf""#,
        r#""dbpath":"/tmp/db""#,
        r#""hookdir":"/tmp/hooks""#,
        r#""print_format":"%n""#,
        r#""file":"/tmp/a.pkg.tar.zst""#,
        r#""url":"http://x/a.pkg.tar.zst""#,
        r#""upgrade":true"#,
        r#""sysupgrade":true"#,
        r#""refresh":true"#,
    ] {
        let at_install = format!(
            r#"{{"protocol_version":1,"action":{{"install":{{"targets":[{{"repo":"extra","name":"a"}}],"providers":[],"transaction":[{{"repo":"extra","name":"a","version":"1-1"}}],{extra}}}}}}}"#
        );
        let at_top = format!(
            r#"{{"protocol_version":1,{extra},"action":{{"update_system":{{"transaction":[{{"repo":"core","name":"a","version":"1-1"}}]}}}}}}"#
        );
        for request in [at_install, at_top] {
            assert_eq!(
                parse_request(request.as_bytes()),
                Err(ProtocolError::Malformed),
                "{extra}"
            );
        }
    }
}

#[test]
fn actions_that_do_not_exist_are_refused() {
    for action in [
        r#"{"remove":{"transaction":[]}}"#,
        r#"{"upgrade_file":{"path":"/tmp/a.pkg.tar.zst"}}"#,
        r#"{"sync":{}}"#,
        r#"{"refresh":{}}"#,
        r#"{"run":{"command":"id"}}"#,
        r#""install""#,
        r#"[]"#,
        r#"null"#,
        r#"{}"#,
    ] {
        let request = format!(r#"{{"protocol_version":1,"action":{action}}}"#);
        assert_eq!(
            parse_request(request.as_bytes()),
            Err(ProtocolError::Malformed),
            "{action}"
        );
    }
}

#[test]
fn protocol_versions_other_than_ours_are_refused() {
    let body = r#""action":{"update_system":{"transaction":[{"repo":"core","name":"a","version":"1-1"}]}}"#;
    for (version, expected) in [
        ("0", ProtocolError::VersionMismatch),
        ("2", ProtocolError::VersionMismatch),
        ("4294967295", ProtocolError::VersionMismatch),
        ("-1", ProtocolError::Malformed),
        ("1.5", ProtocolError::Malformed),
        ("1e9", ProtocolError::Malformed),
        (r#""1""#, ProtocolError::Malformed),
        ("null", ProtocolError::Malformed),
        ("true", ProtocolError::Malformed),
    ] {
        let request = format!(r#"{{"protocol_version":{version},{body}}}"#);
        assert_eq!(
            parse_request(request.as_bytes()),
            Err(expected),
            "version {version}"
        );
    }
}

#[test]
fn broken_json_is_refused() {
    let good = String::from_utf8(install_of("a")).unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("only whitespace", b"   \n".to_vec()),
        ("trailing letters", format!("{good}x").into_bytes()),
        ("two requests", format!("{good}{good}").into_bytes()),
        (
            "two requests on two lines",
            format!("{good}\n{good}").into_bytes(),
        ),
        ("trailing NUL", format!("{good}\0").into_bytes()),
        ("truncated", good.as_bytes()[..good.len() - 1].to_vec()),
        ("single quotes", good.replace('"', "'").into_bytes()),
        (
            "trailing comma",
            good.replacen(r#","protocol_version":1}"#, r#","protocol_version":1,}"#, 1)
                .into_bytes(),
        ),
        ("comment", format!("/* x */{good}").into_bytes()),
        (
            "byte order mark",
            [&[0xef, 0xbb, 0xbf][..], good.as_bytes()].concat(),
        ),
        (
            "invalid UTF-8",
            [&[0xff, 0xfe][..], good.as_bytes()].concat(),
        ),
        (
            "repeated protocol_version",
            good.replacen(
                r#""protocol_version":1"#,
                r#""protocol_version":1,"protocol_version":1"#,
                1,
            )
            .into_bytes(),
        ),
        (
            "repeated action",
            good.replacen(r#""action":"#, r#""action":{},"action":"#, 1)
                .into_bytes(),
        ),
    ];
    for (label, bytes) in cases {
        assert_eq!(
            parse_request(&bytes),
            Err(ProtocolError::Malformed),
            "{label}"
        );
    }
}

#[test]
fn deep_nesting_is_refused_without_a_crash() {
    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let cases = [
        deep.clone(),
        format!(r#"{{"protocol_version":1,"action":{deep}}}"#),
        format!(r#"{{"protocol_version":1,"action":{{"install":{{"targets":{deep}}}}}}}"#),
    ];
    for request in cases {
        assert_eq!(
            parse_request(request.as_bytes()),
            Err(ProtocolError::Malformed)
        );
    }
}

#[test]
fn a_request_over_two_mebibytes_is_refused_before_it_is_read() {
    let mut big = install_of("a");
    big.resize(MAX_REQUEST_BYTES + 1, b' ');
    assert_eq!(parse_request(&big), Err(ProtocolError::TooLarge));
    // The same request padded to exactly the cap is read normally.
    let mut at_cap = install_of("a");
    at_cap.resize(MAX_REQUEST_BYTES, b' ');
    assert!(parse_request(&at_cap).is_ok());
}

#[test]
fn no_refusal_repeats_what_was_sent() {
    let marker = "SECRETMARKER";
    for bytes in [
        install_of(&format!("--{marker}")),
        install_of(&format!("{marker}/../../etc/shadow")),
        format!(r#"{{"protocol_version":1,"{marker}":1}}"#).into_bytes(),
        format!(r#"{{"protocol_version":2,"{marker}":1}}"#).into_bytes(),
    ] {
        let error = parse_request(&bytes).unwrap_err();
        assert!(!format!("{error:?}").contains(marker));
    }
}

// ---- what the password dialog can be made to show (threat T-02, question 21) --------------------------------------
//
// The KDE agent draws the polkit message as rich text (bold, links, `&amp;` decoded), keeps new lines, reverses
// right-to-left text and grows with the text until the password field and the buttons are off the screen (hand test of
// 17 Nov 2026, threat model question 21). Any process can put its own text in argv, so the dialog of a *hostile* caller
// cannot be cleaned. What can be kept clean is the **genuine** dialog: the text the app puts in argv is `summary()` of
// a request that passed validation, so these tests pin that such a text (1) uses only characters that cannot make
// markup, a link, a new line or reversed text, and (2) is short enough to keep the buttons on screen.

use std::collections::BTreeSet;
use tidewise_helper_protocol::{
    MAX_NAME_BYTES, MAX_PROVIDERS, MAX_SUMMARY_BYTES, Request, summary,
};
use tidewise_names::is_valid_package_name;

/// The characters a summary of an accepted request may hold. Derived, not typed:
///  - what a package or repository name may hold is read from the allow-list itself (an ASCII character `c` is allowed
///    when `"a" + c` is a legal name: letters, digits and `@ . _ + -`);
///  - what `summary.rs` writes by itself: its fixed words are letters, its counts are digits, and its separators are a
///    space, `/`, `(`, `)` and the `.` of `...` (the `.` is also a name character).
fn safe_summary_characters() -> BTreeSet<char> {
    let mut set: BTreeSet<char> = (0u8..=127)
        .map(char::from)
        .filter(|c| is_valid_package_name(&format!("a{c}")))
        .collect();
    set.extend(" /().".chars());
    set
}

#[test]
fn the_names_allow_list_holds_no_markup_quote_substitution_or_control_character() {
    let safe = safe_summary_characters();
    // The characters the dialog would interpret or that would change its shape.
    let dangerous = "<>&\"'`$\\%;|{}[]!#*?~^=,:\u{7f}\u{202e}\u{202c}\u{2066}\u{2069}";
    for c in dangerous.chars().chain((0u8..32).map(char::from)) {
        assert!(
            !safe.contains(&c),
            "the allow-list now lets {c:?} into a summary"
        );
    }
    // And only plain ASCII at all.
    assert!(safe.iter().all(|c| c.is_ascii_graphic() || *c == ' '));
}

/// A valid request, as JSON: `targets` and `providers` are `(repo, name)` and `(dependency, repo, name)`; the confirmed
/// list holds all of them.
fn valid_install(
    action: &str,
    targets: &[(String, String)],
    providers: &[(String, String, String)],
) -> Vec<u8> {
    let mut transaction: Vec<Value> = targets.iter().map(|(r, n)| line_of(r, n)).collect();
    transaction.extend(providers.iter().map(|(_, r, n)| line_of(r, n)));
    json!({"protocol_version": 1, "action": {action: {
        "targets": targets.iter().map(|(r, n)| json!({"repo": r, "name": n})).collect::<Vec<_>>(),
        "providers": providers.iter()
            .map(|(d, r, n)| json!({"dependency": d, "chosen": {"repo": r, "name": n}}))
            .collect::<Vec<_>>(),
        "transaction": transaction,
    }}})
    .to_string()
    .into_bytes()
}

fn line_of(repo: &str, name: &str) -> Value {
    json!({"repo": repo, "name": name, "version": "1-1"})
}

fn valid_update(lines: usize) -> Vec<u8> {
    let transaction: Vec<Value> = (0..lines)
        .map(|n| line_of("core", &format!("p{n}")))
        .collect();
    json!({"protocol_version": 1, "action": {"update_system": {"transaction": transaction}}})
        .to_string()
        .into_bytes()
}

/// Valid requests that reach every branch of `summary()`.
fn valid_requests() -> Vec<(&'static str, Vec<u8>)> {
    let max_name = format!("a{}", "b".repeat(MAX_NAME_BYTES - 1));
    let all_characters = "a@._+-9Z".to_string();
    let repo_64 = "r".repeat(64);
    let many: Vec<(String, String)> = (0..MAX_TARGETS)
        .map(|n| ("extra".to_string(), format!("package-number-{n}")))
        .collect();
    let many_providers: Vec<(String, String, String)> = (0..MAX_PROVIDERS)
        .map(|n| {
            (
                format!("dep{n}"),
                "extra".to_string(),
                format!("provider-{n}"),
            )
        })
        .collect();
    let one = |repo: &str, name: &str| vec![(repo.to_string(), name.to_string())];
    vec![
        (
            "fixture: install",
            include_str!("../../../tests/fixtures/helper-protocol/install.json")
                .as_bytes()
                .to_vec(),
        ),
        (
            "fixture: update system",
            include_str!("../../../tests/fixtures/helper-protocol/update-system.json")
                .as_bytes()
                .to_vec(),
        ),
        (
            "fixture: update system and install",
            include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json")
                .as_bytes()
                .to_vec(),
        ),
        (
            "every character a name may hold",
            valid_install("install", &one("extra", &all_characters), &[]),
        ),
        (
            "a name of the longest length (the ... case)",
            valid_install("install", &one("extra", &max_name), &[]),
        ),
        (
            "a repository of 64 bytes",
            valid_install("install", &one(&repo_64, "dbeaver"), &[]),
        ),
        (
            "100 targets (and K more)",
            valid_install("install", &many, &[]),
        ),
        (
            "100 providers with one target (counted)",
            valid_install("install", &one("extra", "dbeaver"), &many_providers),
        ),
        (
            "100 targets and 100 providers",
            valid_install("update_system_and_install", &many, &many_providers),
        ),
        ("an update of 5000 lines", valid_update(MAX_TRANSACTION)),
    ]
}

#[test]
fn the_summary_of_every_accepted_request_uses_only_safe_characters_and_fits_the_dialog() {
    // The cap behind "the buttons stay on screen": a 192-byte summary was shown whole on KDE (15 Nov 2026, hand test),
    // while 200 lines and 20 KB pushed the password field and the buttons off the screen (17 Nov 2026). If this number
    // changes, the hand test of question 21 has to be done again.
    assert_eq!(MAX_SUMMARY_BYTES, 200);

    let safe = safe_summary_characters();
    for (label, bytes) in valid_requests() {
        let request = parse_request(&bytes)
            .unwrap_or_else(|error| panic!("{label}: the request is not valid: {error:?}"));
        let text = summary(&request);
        assert!(!text.is_empty(), "{label}");
        assert!(
            text.len() <= MAX_SUMMARY_BYTES,
            "{label}: {} bytes: {text}",
            text.len()
        );
        let unsafe_characters: Vec<char> = text.chars().filter(|c| !safe.contains(c)).collect();
        assert!(
            unsafe_characters.is_empty(),
            "{label}: {unsafe_characters:?} in {text:?}"
        );
    }
}

#[test]
fn a_request_with_markup_in_a_name_is_refused_so_its_summary_never_reaches_a_dialog() {
    let mut names = hostile_names();
    // What the hand test put in argv, as a package name.
    for (label, name) in [
        ("bold markup", "<b>bold</b>"),
        ("link markup", r#"<a href="https://example.org">click</a>"#),
        ("entity", "a&amp;b"),
        ("double quote", "a\"b"),
        ("single quote", "a'b"),
        ("polkit substitution", "$(user)"),
        ("percent", "a%nb"),
    ] {
        names.push((label, name.to_string()));
    }
    for (label, name) in &names {
        assert_eq!(
            parse_request(&install_of(name)),
            Err(invalid(Field::Targets, 0, Reason::BadPackageName)),
            "{label}"
        );
    }

    // `summary()` alone is NOT a sanitiser: it only keeps printable ASCII, so markup survives it. What keeps the
    // dialog clean is that the helper (and the app, which must call it the same way: M4) validates first.
    let unchecked: Request =
        serde_json::from_slice(&install_of("<b>x</b>")).expect("serde accepts it");
    assert!(
        summary(&unchecked).contains("<b>"),
        "{}",
        summary(&unchecked)
    );
}
