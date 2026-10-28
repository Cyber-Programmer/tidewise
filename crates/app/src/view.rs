//! Copies plain `perf` data into the Slint globals.

use slint::{Color, ComponentHandle, ModelRc, VecModel};

use crate::chart;
use crate::perf::{PerfData, View};
use crate::{Fact, MainWindow, PerfMetric, PerfState, TopRow};

/// Data colours in tab order: processor, memory, network, disk (Theme.data-*).
const COLORS: [(u8, u8, u8); 4] = [
    (0x7A, 0xA7, 0xFF),
    (0xF2, 0xB8, 0x4B),
    (0x5D, 0xDC, 0xB0),
    (0xC3, 0xA2, 0xFF),
];

/// Width in px of the chart inside the panel for a window `window_width` px wide, from the
/// artboard: 220 sidebar, 36 padding each side, content at most 1120, 260 tabs, 20 gap, 25 panel inset.
pub fn chart_width(window_width: f32) -> f32 {
    let content = (window_width - 220.0 - 72.0).clamp(300.0, 1120.0);
    (content - 260.0 - 20.0 - 50.0).max(100.0)
}

pub fn apply_perf(ui: &MainWindow, data: &PerfData) {
    let width = chart_width(ui.window().size().width as f32 / ui.window().scale_factor()) as f64;
    let metrics: Vec<PerfMetric> = data
        .metrics
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (r, g, b) = COLORS[i.min(3)];
            PerfMetric {
                label: m.label.as_str().into(),
                value: m.value.as_str().into(),
                unit: m.unit.as_str().into(),
                mood: m.mood.as_str().into(),
                axis: m.axis.as_str().into(),
                headline: m.headline.as_str().into(),
                explain: m.explain.as_str().into(),
                color: Color::from_rgb_u8(r, g, b),
                spark: chart::spark_path(&m.vals).into(),
                line: chart::line_path(&m.vals, width, 220.0, 5.5).into(),
                area: chart::area_path(&m.vals, width, 220.0, 5.5).into(),
                facts: ModelRc::new(VecModel::from(
                    m.facts
                        .iter()
                        .map(|f| Fact {
                            k: f.k.as_str().into(),
                            v: f.v.as_str().into(),
                        })
                        .collect::<Vec<_>>(),
                )),
                top: ModelRc::new(VecModel::from(
                    m.top
                        .iter()
                        .map(|t| TopRow {
                            name: t.name.as_str().into(),
                            value: t.value.as_str().into(),
                            width: t.width as f32,
                        })
                        .collect::<Vec<_>>(),
                )),
            }
        })
        .collect();
    let state = ui.global::<PerfState>();
    state.set_chart_width(width as f32);
    state.set_subtitle(data.subtitle.as_str().into());
    state.set_metrics(ModelRc::new(VecModel::from(metrics)));
    state.set_threads(ModelRc::new(VecModel::from(
        data.threads.iter().map(|t| *t as f32).collect::<Vec<_>>(),
    )));
}

pub fn apply_view(ui: &MainWindow, view: &View) {
    let state = ui.global::<PerfState>();
    state.set_selected(view.metric as i32);
    state.set_details(view.details);
}

#[cfg(test)]
mod tests {
    use super::chart_width;

    #[test]
    fn chart_width_matches_the_artboard_at_1360() {
        assert_eq!(chart_width(1360.0), 738.0);
    }

    #[test]
    fn chart_width_stops_growing_at_the_content_limit() {
        assert_eq!(chart_width(3000.0), 1120.0 - 330.0);
    }
}
