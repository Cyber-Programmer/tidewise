//! Layout tokens that more than one screen must agree on live in ui/theme.slint (docs/design/tokens.md, "Layout
//! rules"). Reads the real file, so this test can't drift from the app.

/// The pixel value of `out property <length> name: Npx;` in ui/theme.slint.
fn length(name: &str) -> Option<f64> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/ui/theme.slint");
    let source = std::fs::read_to_string(path).expect("read ui/theme.slint");
    let prefix = format!("out property <length> {name}: ");
    source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|rest| rest.strip_suffix("px;"))
        .and_then(|number| number.parse().ok())
}

#[test]
fn content_stretches_to_a_max_width_between_960_and_1040_px() {
    let width = length("content-max-width").expect("a content-max-width token in theme.slint");
    assert!((960.0..=1040.0).contains(&width), "got {width}px");
}
