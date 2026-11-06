//! Colour contrast (WCAG 2.x): text 4.5:1, large text and graphics 3:1, in both themes.
//! (A) pairs named by `docs/design/tokens.md`; (B) the label/background pairs the UI really uses.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn channel(hex: &str, at: usize) -> f64 {
    let v = u8::from_str_radix(&hex[at..at + 2], 16).unwrap() as f64 / 255.0;
    if v <= 0.03928 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    0.2126 * channel(h, 0) + 0.7152 * channel(h, 2) + 0.0722 * channel(h, 4)
}

fn ratio(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn hexes_in(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(i) = rest.find('#') {
        let digits: String = rest[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if digits.len() == 6 {
            out.push(format!("#{}", digits.to_uppercase()));
        }
        rest = &rest[i + 1..];
    }
    out
}

/// Row name -> for each value column (dark, light) the hex codes in it, in order.
fn token_rows() -> HashMap<String, Vec<Vec<String>>> {
    let mut rows = HashMap::new();
    for line in read("docs/design/tokens.md").lines() {
        if !line.starts_with('|') || line.starts_with("| ---") {
            continue;
        }
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() >= 2 {
            rows.insert(
                cells[0].to_string(),
                cells[1..].iter().map(|c| hexes_in(c)).collect(),
            );
        }
    }
    rows
}

const THEMES: [&str; 2] = ["dark", "light"];

#[test]
fn token_pairs_meet_wcag_in_both_themes() {
    let rows = token_rows();
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0;
    let mut check = |theme: &str, what: &str, fg: &str, bg: &str, min: f64| {
        checked += 1;
        let r = ratio(fg, bg);
        if r < min {
            failures.push(format!(
                "{theme}: {what} {fg} on {bg} = {r:.2}:1 (needs {min})"
            ));
        }
    };
    for (i, theme) in THEMES.iter().enumerate() {
        let one = |name: &str| -> Vec<String> { rows[name][i].clone() };
        let backgrounds = ["bg", "sidebar", "surface", "surface-sunk"];
        for fg in ["text", "text-2", "text-3"] {
            for bg in backgrounds {
                check(
                    theme,
                    &format!("{fg} on {bg}"),
                    &one(fg)[0],
                    &one(bg)[0],
                    4.5,
                );
            }
        }
        let accent = &one("accent (as text)")[0];
        for bg in ["bg", "surface"] {
            check(
                theme,
                &format!("accent text on {bg}"),
                accent,
                &one(bg)[0],
                4.5,
            );
        }
        check(
            theme,
            "accent text on its tint",
            accent,
            &one("accent tint bg")[0],
            4.5,
        );
        let fill = one("accent (fill)");
        check(theme, "text on the accent fill", &fill[1], &fill[0], 4.5);
        let warn = one("warn");
        check(
            theme,
            "warn text on surface",
            &warn[0],
            &one("surface")[0],
            4.5,
        );
        // Amber on its own tint is for icons and borders (graphics), not small text.
        check(theme, "warn graphic on its tint", &warn[0], &warn[1], 3.0);
        let danger = one("danger");
        check(
            theme,
            "danger text on surface",
            &danger[0],
            &one("surface")[0],
            4.5,
        );
        check(
            theme,
            "text on the danger fill",
            &danger[1],
            &danger[0],
            4.5,
        );
        if let Some(tint) = danger.get(2) {
            check(theme, "danger text on its tint", &danger[0], tint, 4.5);
        }
        let data = one("data: cpu / memory / disk / network");
        for (name, c) in ["cpu", "memory", "disk"].iter().zip(&data) {
            check(
                theme,
                &format!("{name} chart colour"),
                c,
                &one("surface")[0],
                3.0,
            );
        }
    }
    // Rows that only exist for the dark theme.
    let dark = |name: &str| -> Vec<String> { rows[name][0].clone() };
    for bg in ["sidebar", "nav-active-bg"] {
        check(
            "dark",
            &format!("nav-text on {bg}"),
            &dark("nav-text")[0],
            &dark(bg)[0],
            4.5,
        );
    }
    for bg in ["bg", "surface"] {
        check(
            "dark",
            &format!("text-soft on {bg}"),
            &dark("text-soft")[0],
            &dark(bg)[0],
            4.5,
        );
    }
    let chips = dark("source chips: System / Flatpak / AppImage / Snap");
    for pair in chips.chunks(2) {
        check("dark", "source chip", &pair[0], &pair[1], 4.5);
    }
    assert_eq!(
        checked, 55,
        "pair count changed: did the table format change?"
    );
    assert!(
        failures.is_empty(),
        "{} failing pairs:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `name -> (dark hex, light hex)` for every `out property <color> name: dark ? #A : #B;` of theme.slint.
fn theme_colours() -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for line in read("crates/app/ui/theme.slint").lines() {
        let Some(rest) = line.trim().strip_prefix("out property <color> ") else {
            continue;
        };
        let Some((name, expr)) = rest.split_once(':') else {
            continue;
        };
        let hs = hexes_in(expr);
        if expr.contains("dark ?") && hs.len() == 2 {
            map.insert(name.trim().to_string(), (hs[0].clone(), hs[1].clone()));
        }
    }
    map
}

fn theme_tokens_in(expr: &str) -> Vec<String> {
    expr.split("Theme.")
        .skip(1)
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect()
        })
        .collect()
}

#[test]
fn chip_labels_are_readable_on_their_backgrounds_in_both_themes() {
    let colours = theme_colours();
    assert!(
        colours.len() > 30,
        "theme.slint colours not parsed ({})",
        colours.len()
    );
    let mut failures = Vec::new();
    let mut pairs = 0;
    let dir = root().join("crates/app/ui");
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("slint") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        for (n, line) in fs::read_to_string(&path).unwrap().lines().enumerate() {
            let (Some(f), Some(b)) = (line.find(" fg:"), line.find(" bg:")) else {
                continue;
            };
            let fg_expr = line[f + 4..].split(';').next().unwrap_or("");
            let bg_expr = line[b + 4..].split(';').next().unwrap_or("");
            let (fgs, bgs) = (theme_tokens_in(fg_expr), theme_tokens_in(bg_expr));
            assert_eq!(
                fgs.len(),
                bgs.len(),
                "{file}:{}: fg and bg do not pair up",
                n + 1
            );
            for (fg, bg) in fgs.iter().zip(&bgs) {
                let (Some(fc), Some(bc)) = (colours.get(fg), colours.get(bg)) else {
                    failures.push(format!(
                        "{file}:{}: {fg} or {bg} not found in theme.slint",
                        n + 1
                    ));
                    continue;
                };
                for (theme, f, b) in [("dark", &fc.0, &bc.0), ("light", &fc.1, &bc.1)] {
                    pairs += 1;
                    let r = ratio(f, b);
                    if r < 4.5 {
                        failures.push(format!(
                            "{file}:{}: {theme}: {fg} on {bg} = {r:.2}:1 (needs 4.5)",
                            n + 1
                        ));
                    }
                }
            }
        }
    }
    assert!(pairs >= 2, "no chip pairs found: did the markup change?");
    assert!(
        failures.is_empty(),
        "{} failing pairs:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
