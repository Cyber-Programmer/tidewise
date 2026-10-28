//! SVG path strings for the Performance chart and the tab sparklines.

const POINTS: usize = 60;

/// Line through `values` (each 0..=100) in a `width`×`height` box with `pad` above and below,
/// right-aligned so a short history grows in from the right edge. Empty input gives "".
pub fn line_path(values: &[f64], width: f64, height: f64, pad: f64) -> String {
    let n = values.len();
    let mut out = String::new();
    for (i, v) in values.iter().enumerate() {
        let x = (POINTS - n + i) as f64 * width / (POINTS - 1) as f64;
        let y = height - pad - v.clamp(0.0, 100.0) / 100.0 * (height - pad * 2.0);
        out.push_str(if i == 0 { "M" } else { " L" });
        out.push_str(&format!("{x:.1} {y:.1}"));
    }
    out
}

/// The same line closed down to the baseline, for the soft fill under the chart.
pub fn area_path(values: &[f64], width: f64, height: f64, pad: f64) -> String {
    if values.is_empty() {
        return String::new();
    }
    let x0 = (POINTS - values.len()) as f64 * width / (POINTS - 1) as f64;
    let base = height - pad;
    format!(
        "M{x0:.1} {base:.1} L{} L{width:.1} {base:.1} Z",
        line_path(values, width, height, pad).trim_start_matches('M')
    )
}

/// The last 30 samples across a 120×32 box, as in the artboard's tabs.
pub fn spark_path(values: &[f64]) -> String {
    let tail = &values[values.len().saturating_sub(30)..];
    let n = tail.len();
    let mut out = String::new();
    for (i, v) in tail.iter().enumerate() {
        let x = i as f64 * 120.0 / (n.max(2) - 1) as f64;
        let y = 30.0 - v.clamp(0.0, 100.0) / 100.0 * 28.0;
        out.push_str(if i == 0 { "M" } else { " L" });
        out.push_str(&format!("{x:.1} {y:.1}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_gives_empty_paths() {
        assert_eq!(line_path(&[], 600.0, 200.0, 5.0), "");
        assert_eq!(area_path(&[], 600.0, 200.0, 5.0), "");
        assert_eq!(spark_path(&[]), "");
    }

    #[test]
    fn full_history_spans_the_width() {
        let p = line_path(&[0.0; POINTS], 590.0, 200.0, 5.0);
        assert!(p.starts_with("M0.0 195.0"));
        assert!(p.ends_with("L590.0 195.0"));
    }

    #[test]
    fn short_history_is_right_aligned() {
        assert_eq!(
            line_path(&[100.0, 100.0], 590.0, 200.0, 5.0),
            "M580.0 5.0 L590.0 5.0"
        );
    }

    #[test]
    fn values_are_clamped_to_the_chart() {
        let p = line_path(&[900.0, -3.0], 590.0, 200.0, 5.0);
        assert!(p.contains(" 5.0") && p.contains(" 195.0"));
    }

    #[test]
    fn area_is_the_line_closed_to_the_baseline() {
        let p = area_path(&[50.0; POINTS], 600.0, 200.0, 5.0);
        assert!(p.starts_with("M0.0 195.0 L0.0 100.0"));
        assert!(p.ends_with("L600.0 195.0 Z"));
    }

    #[test]
    fn sparkline_uses_the_last_30_samples() {
        let mut v = vec![0.0; 40];
        v.extend([100.0; 30]);
        let p = spark_path(&v);
        assert!(p.starts_with("M0.0 2.0"), "{p}");
        assert!(p.ends_with("L120.0 2.0"), "{p}");
    }
}
