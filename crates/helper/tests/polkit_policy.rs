//! The polkit policy as a test (threats T-01, T-02, T-03; golden rules 2 and 6; decision 0005).
//!
//! `packaging/polkit/` must hold exactly one policy file with exactly one action, and that action must say what the
//! human agreed to: the helper's absolute path, admin authentication every time (never `_keep`), nobody without a
//! session, and a message that shows the command line (the summary the helper checks). No polkit `.rules` files
//! (JavaScript) anywhere in `packaging/`. XML comments are ignored, so the file may explain itself.
//!
//! Plain text checks, no XML crate. That the file is valid against polkit's DTD, and that polkitd loads it, is
//! checked in the container (`tests/container/tests/polkit.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use tidewise_helper::args::HELPER_PATH;

const POLICY_NAME: &str = "io.github.tidewise.helper.policy";

fn packaging() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging")
}

fn policy_dir() -> PathBuf {
    packaging().join("polkit")
}

/// The policy file's text with every `<!-- ... -->` comment removed.
fn policy() -> String {
    let path = policy_dir().join(POLICY_NAME);
    assert!(path.is_file(), "missing policy file: {}", path.display());
    let text = fs::read_to_string(&path).unwrap();
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("-->")
            .expect("an XML comment is never closed");
        rest = &rest[start + end + 3..];
    }
    out.push_str(rest);
    out
}

/// The text of every `<tag>...</tag>` (tag without attributes), in order.
fn texts_of(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let end = after.find(&close).expect("a tag is never closed");
        found.push(after[..end].trim().to_string());
        rest = &after[end + close.len()..];
    }
    found
}

/// The text of every `<annotate key="KEY">...</annotate>`.
fn annotations(xml: &str, key: &str) -> Vec<String> {
    let open = format!("<annotate key=\"{key}\">");
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let end = after.find("</annotate>").expect("annotate is never closed");
        found.push(after[..end].trim().to_string());
        rest = &after[end..];
    }
    found
}

/// Every file under `dir`, recursively.
fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[test]
fn the_polkit_folder_holds_exactly_the_one_policy_file() {
    let dir = policy_dir();
    assert!(dir.is_dir(), "missing folder: {}", dir.display());
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec![POLICY_NAME.to_string()]);
}

#[test]
fn there_are_no_polkit_rules_files_anywhere_in_packaging() {
    let mut files = Vec::new();
    files_under(&packaging(), &mut files);
    let rules: Vec<_> = files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "rules"))
        .collect();
    assert!(rules.is_empty(), "polkit .rules files found: {rules:?}");
    // The policy file must exist, or this test would pass on an empty folder.
    assert!(policy_dir().join(POLICY_NAME).is_file());
}

#[test]
fn there_is_one_action_with_the_agreed_id() {
    let xml = policy();
    assert_eq!(xml.matches("<action ").count(), 1, "exactly one action");
    assert!(
        xml.contains("<action id=\"io.github.tidewise.helper\">"),
        "the action id is io.github.tidewise.helper"
    );
}

#[test]
fn the_action_is_for_the_helpers_absolute_path_and_nothing_narrower() {
    let xml = policy();
    assert_eq!(
        annotations(&xml, "org.freedesktop.policykit.exec.path"),
        vec!["/usr/lib/tidewise/tidewise-helper".to_string()]
    );
    // argv1 would make the action match only some invocations; a miss falls back to the stock action.
    assert!(!xml.contains("exec.argv1"));
    // allow_gui keeps DISPLAY and XAUTHORITY for the helper; it needs neither.
    assert!(!xml.contains("allow_gui"));
}

#[test]
fn the_action_has_exactly_one_annotation_and_no_translations() {
    // exec.path is the only annotation: `imply` would hand this action's authorization to other actions, `owner` would
    // let another user's process pass details to polkit, and a translated message would show text this file's other
    // checks never look at (review R-07, 5.4).
    let xml = policy();
    assert_eq!(
        xml.matches("<annotate ").count(),
        1,
        "exactly one annotation"
    );
    assert!(!xml.contains("imply"), "imply found");
    assert!(!xml.contains("policykit.owner"), "owner found");
    assert!(!xml.contains("xml:lang"), "a translation found");
    assert!(!xml.contains("gettext"), "gettext found");
}

#[test]
fn the_defaults_are_no_no_and_auth_admin() {
    let xml = policy();
    assert_eq!(texts_of(&xml, "allow_any"), vec!["no".to_string()]);
    assert_eq!(texts_of(&xml, "allow_inactive"), vec!["no".to_string()]);
    assert_eq!(
        texts_of(&xml, "allow_active"),
        vec!["auth_admin".to_string()]
    );
}

#[test]
fn no_authorization_is_ever_kept() {
    // One password per confirmed transaction (golden rule 6); a kept authorization could be used by another process.
    let xml = policy();
    assert!(!xml.contains("_keep"), "auth_*_keep found");
    assert!(!xml.contains("<allow_active>yes<"));
}

#[test]
fn the_dialog_message_shows_the_command_line() {
    // Decision 0005: the summary travels in argv and the user must see it. pkexec fills `command_line`.
    // Not `cmdline_short`: pkexec cuts that one to 80 characters.
    let xml = policy();
    let messages = texts_of(&xml, "message");
    assert_eq!(messages.len(), 1, "one message, no translations");
    assert!(messages[0].contains("$(command_line)"), "{}", messages[0]);
    assert!(!messages[0].contains("cmdline_short"));
}

#[test]
fn the_policy_names_exactly_the_path_the_helper_insists_on() {
    // The helper refuses unless argv[0] is HELPER_PATH, and pkexec matches the policy by the real path of the
    // program. If the two differ, either the helper refuses every request or pkexec falls back to the stock action.
    let xml = policy();
    assert_eq!(
        annotations(&xml, "org.freedesktop.policykit.exec.path"),
        vec![HELPER_PATH.to_string()]
    );
}
