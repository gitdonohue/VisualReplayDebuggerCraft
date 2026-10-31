//! String parameters as timelines: each parameter an entity recorded becomes a [`Track`] of
//! [`Run`]s, stretches of time during which its value (or one level of its value's path) is constant.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use vrd_model::Replay;

use crate::series::{SEPARATORS, valid_range};

/// The part of a value that names a state: the text before the first `|`. What follows is detail,
/// shown on hover.
pub fn path_of(value: &str) -> &str {
    value.split('|').next().unwrap_or("").trim()
}

/// The levels of a value's path: `Game.Movement.Idle` is `Game`, `Movement`, `Idle`.
pub fn segments(value: &str) -> Vec<&str> {
    let path = path_of(value);
    if path.is_empty() { Vec::new() } else { path.split(SEPARATORS).collect() }
}

/// What a block is coloured by: the last level of its path, so that `A.Idle` and `B.Idle` match.
pub fn color_label(label: &str) -> &str {
    label.rsplit(SEPARATORS).next().unwrap_or(label)
}

/// A stretch of time during which a value is constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    pub start: f64,
    /// Equal to `start` for a value that was replaced within the same frame.
    pub end: f64,
    /// Which value, as an index into [`Level::labels`]. 0 means nothing was set.
    pub key: u32,
}

/// One way of cutting a track into runs: by whole value, or by one level of the value's path.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    /// The text of a block, by key. `labels[0]` is empty.
    pub labels: Vec<String>,
    /// What a key stands for in full, by key: the whole path up to this level.
    pub paths: Vec<String>,
    /// In time order, contiguous. Includes gaps (key 0), so that neighbouring equal values merge.
    pub runs: Vec<Run>,
    /// How many runs hold a value.
    pub filled: usize,
    /// Whether anything happens: more than one run, so a value appears, changes or goes away.
    pub changes: bool,
}

impl Level {
    fn new(labels: Vec<String>, paths: Vec<String>, runs: Vec<Run>) -> Level {
        let filled = runs.iter().filter(|r| r.key != 0).count();
        let changes = runs.len() > 1;
        Level { labels, paths, runs, filled, changes }
    }
}

/// The values one entity recorded under one parameter name.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub entity: i32,
    pub name: String,
    /// The name without what it shares with similar names of the same entity; what labels the row.
    pub short_name: String,
    /// When the entity exists. Nothing is shown outside this span.
    pub valid: (f64, f64),
    times: Vec<f64>,
    values: Vec<u32>,
    strings: Vec<String>,
    /// Runs of whole values.
    pub whole: Level,
    /// Runs of each level of the values' paths, from the top.
    pub levels: Vec<Level>,
}

impl Track {
    /// `samples` must be in time order.
    pub fn new<S: AsRef<str>>(
        entity: i32,
        name: impl Into<String>,
        samples: impl IntoIterator<Item = (f64, S)>,
        valid: (f64, f64),
    ) -> Track {
        let mut strings = vec![String::new()];
        let mut ids: HashMap<String, u32> = HashMap::from([(String::new(), 0)]);
        let (mut times, mut values) = (Vec::new(), Vec::new());
        for (t, value) in samples {
            if !t.is_finite() {
                continue;
            }
            let value = value.as_ref();
            let id = *ids.entry(value.to_string()).or_insert_with(|| {
                strings.push(value.to_string());
                (strings.len() - 1) as u32
            });
            times.push(t);
            values.push(id);
        }

        let whole_labels: Vec<String> =
            strings.iter().map(|s| if path_of(s).is_empty() { s.clone() } else { path_of(s).to_string() }).collect();
        let whole = Level::new(whole_labels.clone(), whole_labels, runs_of(&times, values.iter().copied(), valid));

        let split: Vec<Vec<&str>> = strings.iter().map(|s| segments(s)).collect();
        let depth = split.iter().map(Vec::len).max().unwrap_or(0);
        let levels = (0..depth)
            .map(|d| {
                let (mut labels, mut paths) = (vec![String::new()], vec![String::new()]);
                let mut by_path: BTreeMap<String, u32> = BTreeMap::new();
                let key_of: Vec<u32> = split
                    .iter()
                    .map(|segs| match segs.get(d) {
                        Some(label) if !label.is_empty() => {
                            let path = segs[..=d].join(".");
                            *by_path.entry(path.clone()).or_insert_with(|| {
                                labels.push((*label).to_string());
                                paths.push(path);
                                (labels.len() - 1) as u32
                            })
                        }
                        _ => 0,
                    })
                    .collect();
                Level::new(labels, paths, runs_of(&times, values.iter().map(|&v| key_of[v as usize]), valid))
            })
            .collect();

        let name = name.into();
        Track { entity, short_name: name.clone(), name, valid, times, values, strings, whole, levels }
    }

    /// The level to draw a row from: `None` is the whole value.
    pub fn level(&self, level: Option<usize>) -> &Level {
        level.and_then(|d| self.levels.get(d)).unwrap_or(&self.whole)
    }

    /// The value in effect at `t`, in full, if there is one.
    pub fn value_at(&self, t: f64) -> Option<&str> {
        if t < self.valid.0 || t > self.valid.1 {
            return None;
        }
        let n = self.times.partition_point(|&s| s <= t);
        let id = *self.values.get(n.checked_sub(1)?)?;
        Some(self.strings[id as usize].as_str()).filter(|s| !s.is_empty())
    }

    /// The track never holds a value.
    pub fn is_empty(&self) -> bool {
        self.whole.filled == 0
    }
}

/// Cuts samples into runs of equal keys, clipped to `valid`.
fn runs_of(times: &[f64], keys: impl Iterator<Item = u32>, valid: (f64, f64)) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (i, key) in keys.enumerate() {
        let start = times[i].max(valid.0);
        if start > valid.1 {
            break;
        }
        let end = times.get(i + 1).copied().unwrap_or(valid.1).min(valid.1).max(start);
        match runs.last_mut() {
            Some(last) if last.key == key => last.end = end,
            _ => runs.push(Run { start, end, key }),
        }
    }
    runs
}

/// Gives each track its short name.
fn shorten(mut tracks: Vec<Track>) -> Vec<Track> {
    let short = crate::short_names(&tracks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>());
    for (track, short) in tracks.iter_mut().zip(short) {
        track.short_name = short;
    }
    tracks
}

/// Every string parameter in a capture, grouped by entity.
#[derive(Debug, Clone, Default)]
pub struct TimelineData {
    tracks: BTreeMap<i32, Vec<Track>>,
    colors: BTreeMap<String, usize>,
}

impl TimelineData {
    pub fn build(replay: &Replay) -> TimelineData {
        let mut data = TimelineData::default();
        for entity in replay.entities().filter(|e| !e.params.is_empty()) {
            let Some(valid) = valid_range(replay, entity.id) else {
                continue;
            };
            let tracks: Vec<Track> = entity
                .params
                .iter()
                .map(|(name, list)| {
                    let samples = list.iter().map(|(frame, v)| (replay.time_for_frame(frame), v.as_str()));
                    Track::new(entity.id, name.clone(), samples, valid)
                })
                .filter(|t| !t.is_empty())
                .collect();
            if !tracks.is_empty() {
                data.tracks.insert(entity.id, shorten(tracks));
            }
        }
        let labels: BTreeSet<&str> = data
            .tracks
            .values()
            .flatten()
            .flat_map(|t| t.whole.labels.iter().chain(t.levels.iter().flat_map(|l| l.labels.iter())))
            .map(|l| color_label(l))
            .filter(|l| !l.is_empty())
            .collect();
        data.colors = labels.into_iter().enumerate().map(|(i, l)| (l.to_string(), i)).collect();
        data
    }

    /// The tracks of an entity, sorted by name. Empty if it recorded no text.
    pub fn tracks(&self, entity: i32) -> &[Track] {
        self.tracks.get(&entity).map_or(&[], Vec::as_slice)
    }

    /// Entities that recorded at least one value, by id.
    pub fn entities(&self) -> impl Iterator<Item = i32> + '_ {
        self.tracks.keys().copied()
    }

    pub fn has_tracks(&self, entity: i32) -> bool {
        self.tracks.contains_key(&entity)
    }

    #[cfg(test)]
    pub(crate) fn set_tracks_for_test(&mut self, entity: i32, tracks: Vec<Track>) {
        self.tracks.insert(entity, tracks);
    }

    /// A stable number for a block label, used to give each label its own colour.
    pub fn color_index(&self, label: &str) -> usize {
        self.colors.get(color_label(label)).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIFE: (f64, f64) = (0.0, 10.0);

    fn track(samples: &[(f64, &str)]) -> Track {
        Track::new(1, "State", samples.iter().copied(), LIFE)
    }

    fn keys(level: &Level) -> Vec<(f64, f64, &str)> {
        level.runs.iter().map(|r| (r.start, r.end, level.labels[r.key as usize].as_str())).collect()
    }

    #[test]
    fn paths_split_into_levels_and_details_are_set_aside() {
        assert_eq!(segments("Game.Movement/Idle"), ["Game", "Movement", "Idle"]);
        assert_eq!(segments("Game.Idle | since 3.2s"), ["Game", "Idle"], "text after | is detail");
        assert!(segments("").is_empty() && segments("  |detail").is_empty());
        assert_eq!(color_label("Game.Character.Attack"), "Attack");
        assert_eq!(color_label("Attack"), "Attack");
    }

    #[test]
    fn whole_values_become_runs_until_the_next_change() {
        let t = track(&[(1.0, "A"), (4.0, "B"), (7.0, "A")]);
        assert_eq!(keys(&t.whole), [(1.0, 4.0, "A"), (4.0, 7.0, "B"), (7.0, 10.0, "A")]);
        assert_eq!(t.whole.runs[0].key, t.whole.runs[2].key, "equal values share a key");
        assert_eq!((t.whole.filled, t.whole.changes), (3, true));
    }

    #[test]
    fn levels_show_one_part_of_the_path_and_merge_what_does_not_change() {
        let t = track(&[(0.0, "Game.Idle"), (2.0, "Game.Run"), (5.0, "Menu.Run"), (8.0, "Menu")]);
        assert_eq!(t.levels.len(), 2);
        assert_eq!(
            keys(&t.levels[0]),
            [(0.0, 5.0, "Game"), (5.0, 10.0, "Menu")],
            "Game.Idle and Game.Run are one block"
        );
        assert_eq!(
            keys(&t.levels[1]),
            [(0.0, 2.0, "Idle"), (2.0, 5.0, "Run"), (5.0, 8.0, "Run"), (8.0, 10.0, "")],
            "Game.Run and Menu.Run are told apart even though both end in Run"
        );
    }

    #[test]
    fn a_child_under_a_different_parent_is_a_different_block() {
        let t = track(&[(0.0, "A.X"), (3.0, "B.X")]);
        let l1 = &t.levels[1];
        assert_eq!(l1.runs.len(), 2, "A.X and B.X are not the same state");
        assert_eq!(l1.paths[l1.runs[0].key as usize], "A.X");
        assert_eq!(l1.paths[l1.runs[1].key as usize], "B.X");
        assert_eq!(l1.labels[l1.runs[0].key as usize], l1.labels[l1.runs[1].key as usize]);
    }

    #[test]
    fn clearing_a_value_leaves_a_gap_and_empty_tracks_are_detected() {
        let t = track(&[(0.0, "A"), (3.0, ""), (6.0, "A")]);
        assert_eq!(keys(&t.whole), [(0.0, 3.0, "A"), (3.0, 6.0, ""), (6.0, 10.0, "A")]);
        assert_eq!(t.whole.filled, 2);
        assert!(!t.is_empty());
        assert!(track(&[(0.0, "")]).is_empty());
        assert!(track(&[]).is_empty());
    }

    #[test]
    fn a_value_replaced_within_a_frame_is_a_zero_length_run() {
        let t = track(&[(0.0, "ok"), (5.0, "started"), (5.0, "ok")]);
        assert_eq!(keys(&t.whole), [(0.0, 5.0, "ok"), (5.0, 5.0, "started"), (5.0, 10.0, "ok")]);
    }

    #[test]
    fn runs_are_clipped_to_the_entitys_lifetime() {
        let t = Track::new(1, "x", [(0.0, "A"), (4.0, "B"), (20.0, "C")], (2.0, 6.0));
        assert_eq!(keys(&t.whole), [(2.0, 4.0, "A"), (4.0, 6.0, "B")]);
        assert_eq!(t.value_at(1.0), None);
        assert_eq!(t.value_at(3.0), Some("A"));
        assert_eq!(t.value_at(7.0), None, "gone after the entity");
    }

    #[test]
    fn value_at_returns_the_whole_value_with_its_detail() {
        let t = track(&[(1.0, "Game.Idle | frames: 12"), (5.0, "")]);
        assert_eq!(t.value_at(0.5), None);
        assert_eq!(t.value_at(2.0), Some("Game.Idle | frames: 12"));
        assert_eq!(t.value_at(6.0), None, "cleared");
    }

    #[test]
    fn detail_changes_split_whole_value_runs_but_not_level_runs() {
        let t = track(&[(0.0, "A.B | 1"), (3.0, "A.B | 2")]);
        assert_eq!(t.whole.runs.len(), 2);
        assert_eq!(t.levels[0].runs.len(), 1);
        assert_eq!(t.levels[1].runs.len(), 1);
    }

    #[test]
    fn paths_without_levels_fall_back_to_the_whole_value() {
        let t = track(&[(0.0, "|only detail")]);
        assert!(t.levels.is_empty());
        assert_eq!(t.level(Some(0)).filled, 1, "asking for a missing level gives the whole value");
        assert_eq!(t.whole.labels[t.whole.runs[0].key as usize], "|only detail");
    }
}
