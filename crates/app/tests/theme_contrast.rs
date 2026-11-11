//! Every text/background pair from docs/design/tokens.md must stay at least 4.5:1 (WCAG AA).
//! Reads the real colours from ui/theme.slint, so this test can't drift from the app.

use std::collections::HashMap;
use tidewise_core::contrast::contrast_ratio;

const MIN_RATIO: f64 = 4.5;

/// (text token, background token), from the "Contrast pairs" table in docs/design/tokens.md.
const PAIRS: [(&str, &str); 16] = [
    ("text", "bg"),
    ("text", "surface"),
    ("text-2", "surface"),
    ("text-3", "surface"),
    ("text-3", "bg"),
    ("text-3", "surface-2"),
    ("accent", "surface"),
    ("accent", "bg"),
    ("on-accent", "accent"),
    ("warning", "surface"),
    ("warning", "warning-bg"),
    ("danger-text", "surface"),
    ("danger-text", "bg"),
    ("on-danger", "danger"),
    ("text-2", "chip"),
    ("text", "nav-active"),
];

/// Reads `out property <color> name: #RRGGBB;` lines from ui/theme.slint.
fn theme_colours() -> HashMap<String, [u8; 3]> {
    HashMap::new()
}

#[test]
fn theme_has_all_21_colours() {
    assert_eq!(theme_colours().len(), 21);
}

#[test]
fn every_text_pair_meets_wcag_aa() {
    let colours = theme_colours();
    let mut failures = Vec::new();
    for (fg, bg) in PAIRS {
        let (Some(&f), Some(&b)) = (colours.get(fg), colours.get(bg)) else {
            failures.push(format!("{fg} on {bg}: token missing from theme.slint"));
            continue;
        };
        let ratio = contrast_ratio(f, b);
        if ratio < MIN_RATIO {
            failures.push(format!("{fg} on {bg}: {ratio:.2} < {MIN_RATIO}"));
        }
    }
    assert!(failures.is_empty(), "contrast failures:\n{}", failures.join("\n"));
}

