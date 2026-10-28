//! Data behind the Performance screen. Plain Rust, no Slint types: demo mode and live mode both
//! produce a `PerfData`, and the UI only draws it.

pub struct Fact {
    pub k: String,
    pub v: String,
}

pub struct TopRow {
    pub name: String,
    pub value: String,
    /// Bar length, 0.0–1.0.
    pub width: f64,
}

pub struct Metric {
    pub label: String,
    pub value: String,
    pub unit: String,
    pub mood: String,
    pub axis: String,
    pub headline: String,
    pub explain: String,
    /// The last 60 samples on a 0–100 scale.
    pub vals: Vec<f64>,
    pub facts: Vec<Fact>,
    pub top: Vec<TopRow>,
}

pub struct PerfData {
    pub subtitle: String,
    /// Processor, Memory, Network, Disk — in that order.
    pub metrics: Vec<Metric>,
    /// Per-thread load in percent, shown under the CPU facts.
    pub threads: Vec<f64>,
}

/// Which tab is selected and whether technical details are open (the `--state` variants).
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    pub metric: usize,
    pub details: bool,
}

impl View {
    pub fn from_state(name: Option<&str>) -> Result<View, String> {
        let (metric, details) = match name {
            None => (0, false),
            Some("details") => (0, true),
            Some("memory") => (1, false),
            Some("memory-details") => (1, true),
            Some("network") => (2, false),
            Some("disk") => (3, false),
            Some(other) => return Err(format!("unknown performance state '{other}'")),
        };
        Ok(View { metric, details })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_map_to_tab_and_details() {
        assert_eq!(
            View::from_state(None),
            Ok(View {
                metric: 0,
                details: false
            })
        );
        assert_eq!(
            View::from_state(Some("details")),
            Ok(View {
                metric: 0,
                details: true
            })
        );
        assert_eq!(
            View::from_state(Some("memory")),
            Ok(View {
                metric: 1,
                details: false
            })
        );
        assert_eq!(
            View::from_state(Some("memory-details")),
            Ok(View {
                metric: 1,
                details: true
            })
        );
        assert_eq!(
            View::from_state(Some("network")),
            Ok(View {
                metric: 2,
                details: false
            })
        );
        assert_eq!(
            View::from_state(Some("disk")),
            Ok(View {
                metric: 3,
                details: false
            })
        );
        assert!(View::from_state(Some("nope")).is_err());
    }
}
