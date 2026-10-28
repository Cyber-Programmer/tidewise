//! `ui/theme.slint` must carry every colour of `docs/design/tokens.md` with the same hex.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn hexes(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let digits: String = text[i + 1..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if digits.len() == 6 {
                out.insert(format!("#{}", digits.to_uppercase()));
            }
        }
        i += 1;
    }
    out
}

/// (name, dark cell, light cell) for each row of the colour table.
fn colour_rows(tokens: &str) -> Vec<(String, String, String)> {
    tokens
        .lines()
        .skip_while(|l| !l.starts_with("## Colour"))
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| l.starts_with('|'))
        .map(|l| {
            let cells: Vec<_> = l.trim_matches('|').split('|').map(str::trim).collect();
            (
                cells[0].to_string(),
                cells[1].to_string(),
                cells[2].to_string(),
            )
        })
        .filter(|(name, _, _)| name != "Token" && !name.starts_with('-'))
        .collect()
}

#[test]
fn every_hex_in_tokens_md_is_in_theme_slint() {
    let theme = hexes(&read("crates/app/ui/theme.slint"));
    let tokens = read("docs/design/tokens.md");
    let wanted = hexes(&tokens);
    assert!(wanted.len() > 30, "tokens.md parse found only {wanted:?}");
    let missing: Vec<_> = wanted.difference(&theme).collect();
    assert!(missing.is_empty(), "missing from theme.slint: {missing:?}");
}

#[test]
fn simple_tokens_have_the_same_dark_and_light_hex() {
    let theme = read("crates/app/ui/theme.slint");
    let rows = colour_rows(&read("docs/design/tokens.md"));
    let mut checked = 0;
    for name in [
        "bg",
        "sidebar",
        "surface",
        "surface-sunk",
        "border",
        "text",
        "text-2",
        "text-3",
    ] {
        let (_, dark, light) = rows
            .iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("no row {name} in tokens.md"));
        let line = format!("property <color> {name}: dark ? {dark} : {light};");
        assert!(theme.contains(&line), "theme.slint should contain `{line}`");
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn fonts_are_imported_from_assets() {
    let theme = read("crates/app/ui/theme.slint");
    for font in [
        "Geist-Regular.ttf",
        "Geist-Medium.ttf",
        "Geist-SemiBold.ttf",
        "Geist-Bold.ttf",
        "GeistMono-Regular.ttf",
        "GeistMono-Medium.ttf",
    ] {
        assert!(
            theme.contains(&format!("assets/fonts/{font}")),
            "{font} not imported"
        );
        assert!(root().join("assets/fonts").join(font).exists());
    }
    assert!(theme.contains("\"Geist Mono\""));
}

#[test]
fn icons_are_copies_of_the_design_icons() {
    let src = root().join("docs/design/icons");
    let dst = root().join("crates/app/ui/icons");
    let mut n = 0;
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        let copy = fs::read(dst.join(name)).unwrap_or_else(|_| panic!("{name:?} not copied"));
        assert_eq!(copy, fs::read(&path).unwrap(), "{name:?} differs");
        n += 1;
    }
    assert!(n >= 17);
}
