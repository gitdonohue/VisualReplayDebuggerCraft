use std::collections::BTreeMap;

use vrd_model::Replay;

/// Characters that separate the levels of a parameter name: `Physics.Queries.Raycast` is depth 2.
pub(crate) const SEPARATORS: [char; 3] = ['.', '\\', '/'];

/// How deeply a parameter name is nested.
pub fn depth(name: &str) -> usize {
    name.matches(SEPARATORS).count()
}

/// Summary statistics of a series' values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub min: f32,
    pub max: f32,
    pub mean: f64,
    pub std_dev: f64,
}

impl Stats {
    fn of(values: &[f32]) -> Stats {
        if values.is_empty() {
            return Stats { min: 0.0, max: 0.0, mean: 0.0, std_dev: 0.0 };
        }
        let n = values.len() as f64;
        let (min, max) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
        let var = values.iter().map(|&v| (f64::from(v) - mean).powi(2)).sum::<f64>() / n;
        Stats { min, max, mean, std_dev: var.sqrt() }
    }
}

/// The value range a series is drawn over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub lo: f32,
    pub hi: f32,
}

impl Scale {
    /// From zero to the series' extremes, and at least one unit tall, so that values in 0..1 (such as
    /// utility scores) keep a common 0..1 scale. With `ignore_outliers`, a spike more than three
    /// standard deviations from the mean is clipped at two, as the original tool's "autoscale" did.
    pub fn for_stats(stats: &Stats, ignore_outliers: bool) -> Scale {
        let mut lo = stats.min.min(0.0);
        let mut hi = stats.max.max(0.0);
        if ignore_outliers {
            let (mean, sd) = (stats.mean as f32, stats.std_dev as f32);
            if stats.max > mean + 3.0 * sd {
                hi = (mean + 2.0 * sd).max(0.0);
            }
            if stats.min < mean - 3.0 * sd {
                lo = (mean - 2.0 * sd).min(0.0);
            }
        }
        let tall_enough = (hi - lo).is_finite() && hi - lo >= 1.0;
        if !tall_enough {
            hi = lo + 1.0;
        }
        Scale { lo, hi }
    }

    /// The smallest scale covering both.
    pub fn union(self, other: Scale) -> Scale {
        Scale { lo: self.lo.min(other.lo), hi: self.hi.max(other.hi) }
    }

    /// Where `v` falls on the scale: 0 at `lo`, 1 at `hi` (not clamped).
    pub fn fraction(&self, v: f32) -> f32 {
        (v - self.lo) / (self.hi - self.lo)
    }
}

/// The values one entity recorded under one name, as a step function of time.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub entity: i32,
    pub name: String,
    pub depth: usize,
    /// When each value was recorded, ascending and unique.
    pub times: Vec<f64>,
    pub values: Vec<f32>,
    /// When the entity exists. Nothing is drawn outside this span.
    pub valid: (f64, f64),
    pub stats: Stats,
}

impl Series {
    /// `samples` must be in time order. A value holds until the next one; of several values at the
    /// same time only the last counts. Non-finite values are dropped.
    pub fn new(
        entity: i32,
        name: impl Into<String>,
        samples: impl IntoIterator<Item = (f64, f32)>,
        valid: (f64, f64),
    ) -> Series {
        let name = name.into();
        let mut times: Vec<f64> = Vec::new();
        let mut values: Vec<f32> = Vec::new();
        for (t, v) in samples {
            if !(t.is_finite() && v.is_finite()) {
                continue;
            }
            if times.last() == Some(&t) {
                *values.last_mut().expect("parallel vectors") = v;
            } else {
                times.push(t);
                values.push(v);
            }
        }
        let stats = Stats::of(&values);
        Series { entity, depth: depth(&name), name, times, values, valid, stats }
    }

    /// The value in effect at `t`, if the series has started and the entity exists then.
    pub fn value_at(&self, t: f64) -> Option<f32> {
        if t < self.valid.0 || t > self.valid.1 {
            return None;
        }
        let n = self.times.partition_point(|&s| s <= t);
        n.checked_sub(1).map(|i| self.values[i])
    }

    pub fn scale(&self, ignore_outliers: bool) -> Scale {
        Scale::for_stats(&self.stats, ignore_outliers)
    }
}

/// When an entity exists, in seconds: from its first frame to the end of its last. Anything it
/// recorded outside that span is not shown.
pub(crate) fn valid_range(replay: &Replay, entity: i32) -> Option<(f64, f64)> {
    let (first, last) = replay.lifetime(entity)?;
    let end = if last >= replay.last_frame() { replay.total_time() } else { replay.time_for_frame(last + 1) };
    Some((replay.time_for_frame(first), end))
}

/// Every numeric series in a capture, grouped by entity.
#[derive(Debug, Clone, Default)]
pub struct GraphData {
    series: BTreeMap<i32, Vec<Series>>,
    names: BTreeMap<String, usize>,
}

impl GraphData {
    pub fn build(replay: &Replay) -> GraphData {
        let mut data = GraphData::default();
        for entity in replay.entities().filter(|e| !e.values.is_empty()) {
            let Some(valid) = valid_range(replay, entity.id) else {
                continue;
            };
            let list: Vec<Series> = entity
                .values
                .iter()
                .map(|(name, samples)| {
                    let samples = samples.iter().map(|(frame, &v)| (replay.time_for_frame(frame), v));
                    Series::new(entity.id, name.clone(), samples, valid)
                })
                .filter(|s| !s.times.is_empty())
                .collect();
            if !list.is_empty() {
                data.series.insert(entity.id, list);
            }
        }
        let mut names: Vec<&String> = data.series.values().flatten().map(|s| &s.name).collect();
        names.sort();
        names.dedup();
        data.names = names.into_iter().enumerate().map(|(i, n)| (n.clone(), i)).collect();
        data
    }

    /// The series of an entity, sorted by name. Empty if it recorded no numbers.
    pub fn series(&self, entity: i32) -> &[Series] {
        self.series.get(&entity).map_or(&[], Vec::as_slice)
    }

    /// Entities that recorded at least one number, by id.
    pub fn entities(&self) -> impl Iterator<Item = i32> + '_ {
        self.series.keys().copied()
    }

    pub fn has_series(&self, entity: i32) -> bool {
        self.series.contains_key(&entity)
    }

    /// A stable number for a series name, used to give each name its own colour.
    pub fn name_index(&self, name: &str) -> usize {
        self.names.get(name).copied().unwrap_or(0)
    }
}

/// Matches series names: case-insensitive, `|` separates alternatives, empty matches everything.
#[derive(Debug, Clone, Default)]
pub struct NameFilter {
    alternatives: Vec<String>,
}

impl NameFilter {
    pub fn new(text: &str) -> NameFilter {
        NameFilter {
            alternatives: text.split('|').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty()
    }

    pub fn matches(&self, name: &str) -> bool {
        if self.alternatives.is_empty() {
            return true;
        }
        let name = name.to_lowercase();
        self.alternatives.iter().any(|a| name.contains(a.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_counts_separators() {
        assert_eq!(depth("FPS"), 0);
        assert_eq!(depth("Physics_QueriesByType.Raycast"), 1);
        assert_eq!(depth("a/b\\c.d"), 3);
    }

    #[test]
    fn series_dedupes_same_time_and_drops_garbage() {
        let s = Series::new(1, "x", [(0.0, 1.0), (1.0, 2.0), (1.0, 3.0), (2.0, f32::NAN), (3.0, 4.0)], (0.0, 10.0));
        assert_eq!(s.times, [0.0, 1.0, 3.0]);
        assert_eq!(s.values, [1.0, 3.0, 4.0]);
        assert_eq!(s.stats.min, 1.0);
        assert_eq!(s.stats.max, 4.0);
    }

    #[test]
    fn value_at_holds_until_the_next_sample_and_respects_the_lifetime() {
        let s = Series::new(1, "x", [(1.0, 10.0), (3.0, 30.0)], (0.5, 5.0));
        assert_eq!(s.value_at(0.9), None, "before the first value");
        assert_eq!(s.value_at(1.0), Some(10.0));
        assert_eq!(s.value_at(2.99), Some(10.0));
        assert_eq!(s.value_at(3.0), Some(30.0));
        assert_eq!(s.value_at(5.0), Some(30.0));
        assert_eq!(s.value_at(5.01), None, "after the entity is gone");
    }

    #[test]
    fn scales_start_at_zero_and_are_at_least_one_tall() {
        let stats = |min, max| Stats { min, max, mean: 0.0, std_dev: 0.0 };
        assert_eq!(Scale::for_stats(&stats(20.0, 60.0), false), Scale { lo: 0.0, hi: 60.0 });
        assert_eq!(Scale::for_stats(&stats(0.1, 0.4), false), Scale { lo: 0.0, hi: 1.0 });
        assert_eq!(Scale::for_stats(&stats(-5.0, 3.0), false), Scale { lo: -5.0, hi: 3.0 });
        assert_eq!(Scale::for_stats(&stats(0.0, 0.0), false), Scale { lo: 0.0, hi: 1.0 });
        let s = Scale { lo: 0.0, hi: 50.0 };
        assert_eq!(s.fraction(25.0), 0.5);
        assert_eq!(s.union(Scale { lo: -10.0, hi: 20.0 }), Scale { lo: -10.0, hi: 50.0 });
    }

    #[test]
    fn ignoring_outliers_clips_spikes_only() {
        let mut values = vec![10.0f32; 200];
        values.push(1000.0);
        let spiky = Series::new(1, "x", values.iter().enumerate().map(|(i, &v)| (i as f64, v)), (0.0, 1e9));
        let clipped = spiky.scale(true);
        assert!(clipped.hi < 200.0 && clipped.hi >= 10.0, "{clipped:?}");
        assert_eq!(spiky.scale(false).hi, 1000.0);

        let smooth = Series::new(1, "y", (0..100).map(|i| (f64::from(i), i as f32)), (0.0, 1e9));
        assert_eq!(smooth.scale(true), smooth.scale(false), "no outliers, nothing to clip");
    }

    #[test]
    fn filter_is_case_insensitive_with_alternatives() {
        let f = NameFilter::new("fps | GPU");
        assert!(f.matches("FPS") && f.matches("GPUTime"));
        assert!(!f.matches("Triangles"));
        assert!(NameFilter::new("  ").matches("anything"));
        assert!(NameFilter::new("").is_empty());
    }
}
