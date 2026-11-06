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

/// (name, dark hex) for each row of the "Extra shades used by the artboards" table.
fn extra_shades(tokens: &str) -> Vec<(String, String)> {
    tokens
        .lines()
        .skip_while(|l| !l.starts_with("### Extra shades"))
        .skip(1)
        .take_while(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with('|'))
        .map(|l| {
            let cells: Vec<_> = l.trim_matches('|').split('|').map(str::trim).collect();
            (cells[0].to_string(), cells[1].to_string())
        })
        .filter(|(name, _)| name != "Token" && !name.starts_with('-'))
        .collect()
}

#[test]
fn extra_shades_use_the_names_and_hex_of_tokens_md() {
    let theme = read("crates/app/ui/theme.slint");
    let shades = extra_shades(&read("docs/design/tokens.md"));
    assert_eq!(shades.len(), 9, "{shades:?}");
    for (name, dark) in &shades {
        // The dark hex must be exactly the one in tokens.md; the light value follows it.
        let line = format!("property <color> {name}: dark ? {dark} : #");
        assert!(
            theme.contains(&line),
            "theme.slint should contain `{line}…`"
        );
    }
}

/// Every `#RRGGBB` in the Light column of the main colour table of tokens.md.
fn light_column_hexes(tokens: &str) -> std::collections::HashSet<String> {
    tokens
        .lines()
        .skip_while(|l| !l.starts_with("## Colour"))
        .skip(1)
        .take_while(|l| !l.starts_with('#'))
        .filter(|l| l.starts_with('|'))
        .filter_map(|l| l.trim_matches('|').split('|').nth(2).map(str::to_string))
        .flat_map(|cell| {
            cell.split('#')
                .skip(1)
                .map(|h| format!("#{}", h.chars().take(6).collect::<String>().to_uppercase()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The light theme may only use colours that tokens.md already defines for it: the "extra shades"
/// have no light values of their own, so they are aliases of defined light tokens (no new hex).
#[test]
fn every_light_value_is_a_colour_that_tokens_md_defines_for_the_light_theme() {
    let theme = read("crates/app/ui/theme.slint");
    let allowed = light_column_hexes(&read("docs/design/tokens.md"));
    assert!(allowed.len() >= 15, "{allowed:?}");
    let mut checked = 0;
    for line in theme.lines().filter(|l| l.contains("out property <color>")) {
        let Some((_, rest)) = line.split_once(": dark ? ") else {
            continue;
        };
        let Some((_, light)) = rest.split_once(" : ") else {
            continue;
        };
        let light = light.trim_end_matches(';').to_uppercase();
        assert!(
            allowed.contains(&light),
            "light value {light} is not in tokens.md: {line}"
        );
        checked += 1;
    }
    assert!(checked >= 50, "only {checked} tokens checked");
}

#[test]
fn old_names_of_the_extra_shades_are_gone_everywhere() {
    for old in [
        "selected-bg",
        "inset-border",
        "divider",
        "track-off",
        "grid-base",
        "row-hover",
    ] {
        for entry in fs::read_dir(root().join("crates/app/ui")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "slint") {
                let text = fs::read_to_string(&path).unwrap();
                let (declared, used) = (format!("property <color> {old}:"), format!("Theme.{old}"));
                assert!(
                    !text.contains(&declared) && !text.contains(&used),
                    "{path:?} still uses the old name `{old}`"
                );
            }
        }
    }
}
