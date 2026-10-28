//! Copies plain `perf` data into the Slint globals.

use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};

use crate::chart;
use crate::home::{self, HomeData};
use crate::perf::{PerfData, View};
use crate::procs::{self, ProcData};
use crate::{
    Fact, HomeChip, HomeState, HomeSuggestion, HomeTile, MainWindow, PerfMetric, PerfState,
    ProcRow, ProcState, TopRow,
};

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

pub fn apply_home(ui: &MainWindow, d: &HomeData) {
    let width =
        home::spark_width(ui.window().size().width as f32 / ui.window().scale_factor()) as f64;
    let tiles: Vec<HomeTile> = d
        .tiles
        .iter()
        .map(|t| HomeTile {
            label: t.label.as_str().into(),
            mood: t.mood.as_str().into(),
            mood_hue: t.mood_hue as i32,
            value: t.value.as_str().into(),
            unit: t.unit.as_str().into(),
            hue: t.hue as i32,
            spark: t
                .spark
                .as_deref()
                .map(|p| home::points_path(p, width))
                .unwrap_or_default()
                .into(),
            has_spark: t.spark.is_some(),
            fraction: t.bar.unwrap_or(0.0) as f32,
            has_bar: t.bar.is_some(),
        })
        .collect();
    let state = ui.global::<HomeState>();
    state.set_last_checked(d.last_checked.as_str().into());
    state.set_spark_width(width as f32);
    state.set_tiles(ModelRc::new(VecModel::from(tiles)));
    state.set_has_scan(d.scan.is_some());
    if let Some(scan) = &d.scan {
        state.set_score(scan.score as i32);
        state.set_ring(home::ring_path(scan.score).into());
        state.set_tag(scan.tag.as_str().into());
        state.set_headline(scan.headline.as_str().into());
        state.set_sub(scan.sub.as_str().into());
        let rows: Vec<HomeSuggestion> = scan
            .suggestions
            .iter()
            .map(|s| HomeSuggestion {
                icon: s.icon as i32,
                title: s.title.as_str().into(),
                body: s.body.as_str().into(),
                action: s.action.as_str().into(),
                page: s.page,
            })
            .collect();
        state.set_suggestions(ModelRc::new(VecModel::from(rows)));
    }
    state.set_has_apps(d.apps.is_some());
    if let Some(apps) = &d.apps {
        state.set_apps_title(apps.title.as_str().into());
        state.set_apps_sub(apps.sub.as_str().into());
        let chips: Vec<HomeChip> = apps
            .chips
            .iter()
            .map(|(label, n)| HomeChip {
                label: label.as_str().into(),
                count: n.to_string().into(),
            })
            .collect();
        state.set_chips(ModelRc::new(VecModel::from(chips)));
    }
}

/// Indexes (below the shorter length) whose rows differ between `old` and `new`.
fn changed_rows<T: PartialEq>(old: &[T], new: &[T]) -> Vec<usize> {
    old.iter()
        .zip(new)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect()
}

/// Bring `current` to `new` by touching only the rows that changed, so Slint re-renders only
/// those. Returns a fresh model when `current` isn't one we made (the caller then sets it).
fn sync_model<T: Clone + PartialEq + 'static>(
    current: &ModelRc<T>,
    new: Vec<T>,
) -> Option<ModelRc<T>> {
    let Some(vec) = current.as_any().downcast_ref::<VecModel<T>>() else {
        return Some(ModelRc::new(VecModel::from(new)));
    };
    let old: Vec<T> = vec.iter().collect();
    for i in changed_rows(&old, &new) {
        vec.set_row_data(i, new[i].clone());
    }
    if new.len() < old.len() {
        // Remove from the end, so the rows that stay keep their items.
        for i in (new.len()..old.len()).rev() {
            vec.remove(i);
        }
    } else {
        for row in new.into_iter().skip(old.len()) {
            vec.push(row);
        }
    }
    None
}

fn proc_row(r: &procs::Row, selected: bool) -> ProcRow {
    ProcRow {
        name: r.name.as_str().into(),
        sub: r.sub.as_str().into(),
        sub_details: r.sub_details.as_str().into(),
        letter: r
            .name
            .chars()
            .next()
            .map(String::from)
            .unwrap_or_default()
            .into(),
        color: Color::from_rgb_u8(r.color.0, r.color.1, r.color.2),
        protected: r.protected,
        cpu: r.cpu.as_str().into(),
        cpu_hot: r.cpu_hot,
        mem: r.mem.as_str().into(),
        mem_hot: r.mem_hot,
        disk: r.disk.as_str().into(),
        net: r.net.as_str().into(),
        note: r.note.as_str().into(),
        cmd: r.cmd.as_str().into(),
        selected,
    }
}

/// Show the rows that pass `filter` and `search`; returns their ids in display order, so a click
/// on row `i` can be mapped back to a process group.
pub fn apply_procs(
    ui: &MainWindow,
    data: &ProcData,
    filter: procs::Filter,
    search: &str,
    selected: &str,
) -> Vec<String> {
    let shown = procs::visible(&data.rows, filter, search);
    let rows: Vec<ProcRow> = shown
        .iter()
        .map(|r| proc_row(r, r.id == selected))
        .collect();
    let state = ui.global::<ProcState>();
    state.set_subtitle(data.subtitle.as_str().into());
    if let Some(model) = sync_model(&state.get_rows(), rows) {
        state.set_rows(model);
    }
    state.set_filter(filter.index());
    let sel = data.rows.iter().find(|r| r.id == selected);
    state.set_has_sel(sel.is_some());
    if let Some(r) = sel {
        state.set_sel(proc_row(r, true));
    }
    shown.iter().map(|r| r.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::{changed_rows, chart_width, sync_model};
    use slint::{Model, ModelRc, VecModel};

    #[test]
    fn only_differing_rows_are_listed() {
        assert_eq!(changed_rows(&[1, 2, 3], &[1, 9, 3]), [1]);
        assert!(changed_rows(&[1, 2], &[1, 2]).is_empty());
        assert_eq!(
            changed_rows(&[1, 2, 3], &[7, 2]),
            [0],
            "extra rows are not 'changed'"
        );
    }

    fn contents(m: &ModelRc<i32>) -> Vec<i32> {
        m.iter().collect()
    }

    #[test]
    fn syncing_updates_grows_and_shrinks_the_same_model() {
        let model = ModelRc::new(VecModel::from(vec![1, 2, 3]));
        assert!(sync_model(&model, vec![1, 5, 3]).is_none());
        assert_eq!(contents(&model), [1, 5, 3]);
        assert!(sync_model(&model, vec![1, 5, 3, 4, 6]).is_none());
        assert_eq!(contents(&model), [1, 5, 3, 4, 6]);
        assert!(sync_model(&model, vec![8]).is_none());
        assert_eq!(contents(&model), [8]);
        assert!(sync_model(&model, vec![]).is_none());
        assert!(contents(&model).is_empty());
    }

    #[test]
    fn a_foreign_model_is_replaced() {
        let fresh = sync_model(&ModelRc::default(), vec![1, 2]).expect("new model");
        assert_eq!(contents(&fresh), [1, 2]);
    }

    #[test]
    fn chart_width_matches_the_artboard_at_1360() {
        assert_eq!(chart_width(1360.0), 738.0);
    }

    #[test]
    fn chart_width_stops_growing_at_the_content_limit() {
        assert_eq!(chart_width(3000.0), 1120.0 - 330.0);
    }
}
