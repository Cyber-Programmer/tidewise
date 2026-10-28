//! Data behind the Home screen. Plain Rust, no Slint types.

use tidewise_core::distro::Tools;

/// Which data colour to use; Slint maps it to `Theme.hue()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    Cpu = 0,
    Memory = 1,
    Disk = 2,
    Network = 3,
    Muted = 4,
}

pub struct Tile {
    pub label: String,
    pub mood: String,
    pub mood_hue: Hue,
    pub value: String,
    pub unit: String,
    pub hue: Hue,
    /// A sparkline on a 200×34 canvas, or None.
    pub spark: Option<Vec<(f64, f64)>>,
    /// A bar filled this far (0.0–1.0), or None.
    pub bar: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Clean = 0,
    Warning = 1,
    Chip = 2,
}

pub struct Suggestion {
    pub icon: Icon,
    pub title: String,
    pub body: String,
    pub action: String,
    /// Sidebar page the row opens.
    pub page: i32,
}

pub struct Scan {
    pub score: u32,
    pub tag: String,
    pub headline: String,
    pub sub: String,
    pub suggestions: Vec<Suggestion>,
}

pub struct Apps {
    pub title: String,
    pub sub: String,
    pub chips: Vec<(String, u32)>,
}

pub struct HomeData {
    pub last_checked: String,
    /// Health score and suggestions; None until scanning exists (live mode).
    pub scan: Option<Scan>,
    pub tiles: Vec<Tile>,
    pub apps: Option<Apps>,
}

/// SVG path for the part of the health ring that is filled, on a 132×132 canvas (radius 56),
/// starting at the top and going clockwise. Empty for a score of 0.
pub fn ring_path(score: u32) -> String {
    let score = score.min(100);
    if score == 0 {
        return String::new();
    }
    if score == 100 {
        return "M66 10 A56 56 0 1 1 66 122 A56 56 0 1 1 66 10".into();
    }
    let angle = score as f64 / 100.0 * std::f64::consts::TAU;
    let (x, y) = (66.0 + 56.0 * angle.sin(), 66.0 - 56.0 * angle.cos());
    let large = u8::from(score > 50);
    format!("M66 10 A56 56 0 {large} 1 {x:.2} {y:.2}")
}

/// Polyline commands for points on the tiles' 200×34 canvas, stretched to `width` px.
pub fn points_path(points: &[(f64, f64)], width: f64) -> String {
    points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            format!(
                "{}{:.1} {y:.1}",
                if i == 0 { "M" } else { " L" },
                x * width / 200.0
            )
        })
        .collect()
}

/// Width in px of a tile's sparkline for a window `window_width` px wide: 220 sidebar, 36 padding
/// each side, content at most 1120, four tiles with 12 gaps, 19 px inside each tile edge.
pub fn spark_width(window_width: f32) -> f32 {
    let content = (window_width - 220.0 - 72.0).clamp(300.0, 1120.0);
    ((content - 36.0) / 4.0 - 38.0).max(40.0)
}

/// Sparkline points for samples on a 0–100 scale: newest on the right.
pub fn spark_points(values: &[f64]) -> Vec<(f64, f64)> {
    let n = values.len();
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                i as f64 * 200.0 / (n.max(2) - 1) as f64,
                31.0 - v.clamp(0.0, 100.0) / 100.0 * 26.0,
            )
        })
        .collect()
}

/// "KDE Plasma · pacman + Flatpak": the line under the distro name in the sidebar.
pub fn distro_detail(desktop: &[String], tools: &Tools) -> String {
    let desktop = desktop.last().map(|d| match d.as_str() {
        "KDE" => "KDE Plasma".to_string(),
        other => other.to_string(),
    });
    let mut tools_shown = Vec::new();
    if let Some(manager) = [
        (tools.pacman, "pacman"),
        (tools.apt, "apt"),
        (tools.dnf, "dnf"),
    ]
    .iter()
    .find(|(present, _)| *present)
    {
        tools_shown.push(manager.1);
    }
    if tools.flatpak {
        tools_shown.push("Flatpak");
    }
    if tools.snap {
        tools_shown.push("Snap");
    }
    [
        desktop,
        (!tools_shown.is_empty()).then(|| tools_shown.join(" + ")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_for_82_points_ends_left_of_top_after_a_large_arc() {
        let p = ring_path(82);
        assert!(p.starts_with("M66 10 A56 56 0 1 1 "), "{p}");
        // 82 % of a turn is 295.2°, which is up and to the left of the centre.
        let end: Vec<f64> = p
            .rsplit_once("1 1 ")
            .unwrap()
            .1
            .split(' ')
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(end[0] < 66.0 && end[1] < 66.0, "{p}");
    }

    #[test]
    fn ring_edges() {
        assert_eq!(ring_path(0), "");
        assert!(ring_path(100).contains("A56 56 0 1 1 66 122"));
        assert_eq!(ring_path(250), ring_path(100), "scores are capped at 100");
        assert!(ring_path(25).starts_with("M66 10 A56 56 0 0 1 122.00 66.00"));
    }

    #[test]
    fn points_become_a_path() {
        assert_eq!(
            points_path(&[(0.0, 28.0), (14.0, 26.0)], 200.0),
            "M0.0 28.0 L14.0 26.0"
        );
        assert_eq!(
            points_path(&[(100.0, 1.0)], 220.0),
            "M110.0 1.0",
            "x is stretched, y is not"
        );
        assert_eq!(points_path(&[], 220.0), "");
    }

    #[test]
    fn spark_width_matches_the_artboard_at_1360() {
        assert_eq!(spark_width(1360.0), 220.0);
    }

    #[test]
    fn spark_points_span_the_canvas() {
        let p = spark_points(&[0.0, 100.0]);
        assert_eq!(p, vec![(0.0, 31.0), (200.0, 5.0)]);
        assert!(spark_points(&[]).is_empty());
        assert_eq!(
            spark_points(&[50.0]).len(),
            1,
            "one sample must not divide by zero"
        );
    }

    fn tools(pacman: bool, apt: bool, flatpak: bool, snap: bool) -> Tools {
        Tools {
            pacman,
            apt,
            dnf: false,
            flatpak,
            snap,
        }
    }

    #[test]
    fn distro_line_names_desktop_and_tools() {
        let kde = vec!["KDE".to_string()];
        assert_eq!(
            distro_detail(&kde, &tools(true, false, true, false)),
            "KDE Plasma · pacman + Flatpak"
        );
        let ubuntu = vec!["ubuntu".to_string(), "GNOME".to_string()];
        assert_eq!(
            distro_detail(&ubuntu, &tools(false, true, true, true)),
            "GNOME · apt + Flatpak + Snap"
        );
    }

    #[test]
    fn distro_line_copes_with_nothing_known() {
        assert_eq!(distro_detail(&[], &tools(false, false, false, false)), "");
        assert_eq!(
            distro_detail(&[], &tools(true, false, false, false)),
            "pacman"
        );
    }
}
