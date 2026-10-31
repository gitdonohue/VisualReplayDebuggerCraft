//! What the properties timelines show: which rows there are, and the blocks in each row for a time
//! window drawn a given number of pixels wide.

use std::collections::BTreeSet;

use crate::NameFilter;
use crate::tracks::{Level, Run, TimelineData};

/// Runs narrower than this many pixels are not drawn one by one but gathered into "busy" blocks.
pub const MIN_BLOCK_PX: f32 = 3.0;

/// What to show.
pub struct RowOptions<'a> {
    /// One row per level of the values' paths. Off: one row per parameter, showing whole values.
    pub by_depth: bool,
    /// Leave out rows whose value never changes.
    pub changes_only: bool,
    pub filter: &'a NameFilter,
    /// Parameter names to leave out.
    pub hidden: &'a BTreeSet<String>,
}

/// One row of blocks: a level of one parameter of one entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// Which of the listed entities.
    pub slot: usize,
    /// Which of that entity's tracks.
    pub track: usize,
    /// Which level of the path, or `None` for whole values.
    pub level: Option<usize>,
    /// The first row of its parameter, which carries the parameter's name.
    pub first: bool,
}

/// The rows of one entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub slot: usize,
    pub rows: Vec<Row>,
}

/// Lays out the rows for `entities` (by id). Entities with no rows to show are left out.
pub fn rows(data: &TimelineData, entities: &[i32], opts: &RowOptions) -> Vec<Section> {
    let mut sections = Vec::new();
    for (slot, &entity) in entities.iter().enumerate() {
        let mut rows = Vec::new();
        for (index, track) in data.tracks(entity).iter().enumerate() {
            if !opts.filter.matches(&track.name) || opts.hidden.contains(&track.name) {
                continue;
            }
            let wanted = |level: &Level| level.filled > 0 && (!opts.changes_only || level.changes);
            let start = rows.len();
            if opts.by_depth && !track.levels.is_empty() {
                for (d, level) in track.levels.iter().enumerate() {
                    if wanted(level) {
                        rows.push(Row { slot, track: index, level: Some(d), first: false });
                    }
                }
            } else if wanted(&track.whole) {
                rows.push(Row { slot, track: index, level: None, first: false });
            }
            if let Some(row) = rows.get_mut(start) {
                row.first = true;
            }
        }
        if !rows.is_empty() {
            sections.push(Section { slot, rows });
        }
    }
    sections
}

/// The runs a row is made of.
pub fn row_level<'a>(data: &'a TimelineData, entities: &[i32], row: &Row) -> &'a Level {
    data.tracks(entities[row.slot])[row.track].level(row.level)
}

/// A stretch of a row, positioned in pixels from the window's left edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub x0: f32,
    pub x1: f32,
    /// The time the block really covers, whatever part of it is in view.
    pub start: f64,
    pub end: f64,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// One value, with its key in the row's level.
    Value { key: u32 },
    /// Several changes too close together to draw one by one (or a single one that took no time).
    /// `key` is the value that held longest.
    Busy { changes: usize, key: u32 },
}

/// The blocks to draw for the part of `runs` inside `t0..t1`, laid out over `width` pixels.
pub fn blocks(runs: &[Run], t0: f64, t1: f64, width: f32) -> Vec<Block> {
    let usable = t0.is_finite() && t1.is_finite() && t1 > t0 && width.is_finite() && width >= 1.0;
    if !usable {
        return Vec::new();
    }
    let scale = f64::from(width) / (t1 - t0);
    let x = |t: f64| (((t - t0) * scale) as f32).clamp(0.0, width);

    let lo = runs.partition_point(|r| r.end < t0);
    let hi = runs.partition_point(|r| r.start <= t1).max(lo);

    let mut out = Vec::new();
    let mut crowd: Vec<Run> = Vec::new();
    let flush = |crowd: &mut Vec<Run>, out: &mut Vec<Block>| {
        let filled = crowd.iter().filter(|r| r.key != 0).count();
        if let (Some(first), Some(last)) = (crowd.first(), crowd.last())
            && filled > 0
        {
            let (start, end) = (first.start, last.end);
            let key = dominant(crowd);
            let single = crowd.len() == 1 && (end - start) * scale >= 1.0;
            let kind = if single { BlockKind::Value { key } } else { BlockKind::Busy { changes: filled, key } };
            out.push(Block { x0: x(start), x1: x(end), start, end, kind });
        }
        crowd.clear();
    };

    for run in &runs[lo..hi] {
        if (run.end - run.start) * scale >= f64::from(MIN_BLOCK_PX) {
            flush(&mut crowd, &mut out);
            if run.key != 0 {
                out.push(Block {
                    x0: x(run.start),
                    x1: x(run.end),
                    start: run.start,
                    end: run.end,
                    kind: BlockKind::Value { key: run.key },
                });
            }
        } else {
            crowd.push(*run);
        }
    }
    flush(&mut crowd, &mut out);
    out
}

/// The value that held for longest.
fn dominant(runs: &[Run]) -> u32 {
    let mut best = (0u32, -1.0f64);
    let mut totals: Vec<(u32, f64)> = Vec::new();
    for r in runs.iter().filter(|r| r.key != 0) {
        match totals.iter_mut().find(|(k, _)| *k == r.key) {
            Some((_, d)) => *d += r.end - r.start,
            None => totals.push((r.key, r.end - r.start)),
        }
    }
    for (key, duration) in totals {
        if duration > best.1 {
            best = (key, duration);
        }
    }
    best.0
}

/// The times at which a value starts or stops being shown in any of `levels`, ascending.
pub fn event_times<'a>(levels: impl IntoIterator<Item = &'a Level>) -> Vec<f64> {
    let mut times: Vec<f64> =
        levels.into_iter().flat_map(|l| l.runs.iter().filter(|r| r.key != 0).flat_map(|r| [r.start, r.end])).collect();
    times.sort_by(f64::total_cmp);
    times.dedup();
    times
}

/// The closest event strictly before (`forward == false`) or after `t`.
pub fn neighbouring_event(events: &[f64], t: f64, forward: bool) -> Option<f64> {
    if forward {
        events.get(events.partition_point(|&e| e <= t)).copied()
    } else {
        events.partition_point(|&e| e < t).checked_sub(1).map(|i| events[i])
    }
}

/// The shortest stretch of any of `levels` that covers `t`.
pub fn smallest_block_at<'a>(levels: impl IntoIterator<Item = &'a Level>, t: f64) -> Option<(f64, f64)> {
    levels
        .into_iter()
        .flat_map(|l| l.runs.iter())
        .filter(|r| r.key != 0 && r.start <= t && t < r.end)
        .map(|r| (r.start, r.end))
        .min_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
}

/// Parameter names often share a long prefix (`#123: Trooper.Slot: Attack`, `#123: Trooper.Slot:
/// Move`). This drops from each name what it shares with its closest sibling, back to the last
/// separator, so that the part that tells them apart is what is shown. A name is left alone if
/// that would leave nothing, or only cut a few characters.
pub fn short_names(names: &[&str]) -> Vec<String> {
    const MIN_CUT: usize = 4;
    let shared = |a: &str, b: &str| a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let closest = names.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, other)| shared(name, other)).max();
            let cut = closest
                .and_then(|n| name.as_bytes()[..n].iter().rposition(|b| matches!(b, b' ' | b'.' | b'/' | b'\\' | b':')))
                .map(|p| p + 1);
            match cut {
                Some(cut) if cut >= MIN_CUT && cut < name.len() => name[cut..].to_string(),
                _ => (*name).to_string(),
            }
        })
        .collect()
}
/// The odd-numbered frames in `t0..t1` as pixel spans, to tell frames apart when zoomed in far enough
/// for each to be at least `min_px` wide. `frame_times[f]` is when frame `f` starts.
pub fn frame_stripes(frame_times: &[f32], t0: f64, t1: f64, width: f32, min_px: f32) -> Vec<(f32, f32)> {
    let usable = t0.is_finite() && t1.is_finite() && t1 > t0 && width >= 1.0;
    if !usable || frame_times.len() < 2 {
        return Vec::new();
    }
    let scale = f64::from(width) / (t1 - t0);
    let first = frame_times.partition_point(|&t| f64::from(t) <= t0).saturating_sub(1);
    let last = frame_times.partition_point(|&t| f64::from(t) < t1).min(frame_times.len() - 1);
    if last <= first {
        return Vec::new();
    }
    let average = (f64::from(frame_times[last]) - f64::from(frame_times[first])) / (last - first) as f64;
    if average * scale < f64::from(min_px) {
        return Vec::new();
    }
    (first..last)
        .filter(|f| f % 2 == 1)
        .map(|f| {
            let x = |t: f32| (((f64::from(t) - t0) * scale) as f32).clamp(0.0, width);
            (x(frame_times[f]), x(frame_times[f + 1]))
        })
        .filter(|(a, b)| b > a)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::Track;

    fn run(start: f64, end: f64, key: u32) -> Run {
        Run { start, end, key }
    }

    #[test]
    fn wide_runs_are_blocks_and_gaps_are_not() {
        let runs = [run(0.0, 4.0, 1), run(4.0, 6.0, 0), run(6.0, 10.0, 2)];
        let b = blocks(&runs, 0.0, 10.0, 100.0);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].x0, b[0].x1, b[0].kind), (0.0, 40.0, BlockKind::Value { key: 1 }));
        assert_eq!((b[1].x0, b[1].x1, b[1].kind), (60.0, 100.0, BlockKind::Value { key: 2 }));
    }

    #[test]
    fn only_what_is_in_view_is_returned_but_blocks_keep_their_real_extent() {
        let runs = [run(0.0, 10.0, 1), run(10.0, 20.0, 2), run(20.0, 30.0, 3)];
        let b = blocks(&runs, 12.0, 18.0, 60.0);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].x0, b[0].x1), (0.0, 60.0), "clamped to the window");
        assert_eq!((b[0].start, b[0].end), (10.0, 20.0), "but still known to span 10..20");
        assert!(blocks(&runs, 40.0, 50.0, 100.0).is_empty());
        assert!(blocks(&runs, 5.0, 5.0, 100.0).is_empty());
        assert!(blocks(&runs, 0.0, 10.0, 0.0).is_empty());
    }

    #[test]
    fn a_run_barely_in_view_is_not_mistaken_for_a_narrow_one() {
        // 100 s long but only the last 0.5 px of it shows: still one wide block.
        let runs = [run(0.0, 100.0, 1)];
        let b = blocks(&runs, 99.9, 120.0, 100.0);
        assert_eq!(b.len(), 1);
        assert!(matches!(b[0].kind, BlockKind::Value { key: 1 }));
    }

    #[test]
    fn many_narrow_runs_become_one_busy_block() {
        // 20 alternating runs of 0.1 s drawn at 10 px/s: each is 1 px.
        let runs: Vec<Run> =
            (0..20).map(|i| run(f64::from(i) * 0.1, f64::from(i + 1) * 0.1, 1 + (i % 2) as u32)).collect();
        let b = blocks(&runs, 0.0, 10.0, 100.0);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].start, 0.0);
        assert!((b[0].end - 2.0).abs() < 1e-9);
        assert!(matches!(b[0].kind, BlockKind::Busy { changes: 20, .. }));
        assert!(b[0].x1 - b[0].x0 >= 19.0);
    }

    #[test]
    fn narrow_runs_do_not_swallow_the_wide_ones_around_them() {
        let runs = [run(0.0, 5.0, 1), run(5.0, 5.1, 2), run(5.1, 5.2, 3), run(5.2, 10.0, 1)];
        let b = blocks(&runs, 0.0, 10.0, 100.0);
        let kinds: Vec<_> = b.iter().map(|b| b.kind).collect();
        assert_eq!(kinds.len(), 3, "{kinds:?}");
        assert!(matches!(kinds[0], BlockKind::Value { key: 1 }));
        assert!(matches!(kinds[1], BlockKind::Busy { changes: 2, .. }));
        assert!(matches!(kinds[2], BlockKind::Value { key: 1 }));
        assert!(b[0].x1 <= b[1].x0 + 1e-3 && b[1].x1 <= b[2].x0 + 1e-3);
    }

    #[test]
    fn an_instant_change_shows_as_a_single_busy_block() {
        let runs = [run(0.0, 5.0, 1), run(5.0, 5.0, 2), run(5.0, 10.0, 1)];
        let b = blocks(&runs, 0.0, 10.0, 100.0);
        assert_eq!(b.len(), 3);
        assert!(matches!(b[1].kind, BlockKind::Busy { changes: 1, key: 2 }));
        assert_eq!((b[1].x0, b[1].x1), (50.0, 50.0));
    }

    #[test]
    fn a_busy_block_is_tinted_by_the_value_that_held_longest() {
        // 1 px, 2.5 px and 1 px wide: all narrow, and the middle value held longest.
        let runs = [run(0.0, 0.01, 1), run(0.01, 0.035, 2), run(0.035, 0.045, 1)];
        let b = blocks(&runs, 0.0, 1.0, 100.0);
        assert!(matches!(b[0].kind, BlockKind::Busy { changes: 3, key: 2 }));
    }

    #[test]
    fn busy_blocks_become_values_when_zoomed_in() {
        let runs: Vec<Run> =
            (0..10).map(|i| run(f64::from(i) * 0.1, f64::from(i + 1) * 0.1, 1 + (i % 2) as u32)).collect();
        assert_eq!(blocks(&runs, 0.0, 1.0, 25.0).len(), 1, "2.5 px each");
        let zoomed = blocks(&runs, 0.0, 0.3, 300.0);
        assert_eq!(zoomed.len(), 3);
        assert!(zoomed.iter().all(|b| matches!(b.kind, BlockKind::Value { .. })));
    }

    fn data() -> (TimelineData, Vec<i32>) {
        let mut data = TimelineData::default();
        let state =
            Track::new(1, "State", [(0.0, "Game.Idle"), (4.0, "Game.Run"), (8.0, "Game.Run | fast")], (0.0, 10.0));
        let team = Track::new(1, "Team", [(0.0, "Blue")], (0.0, 10.0));
        let pad = Track::new(2, "State", [(0.0, "Menu")], (0.0, 10.0));
        data.set_tracks_for_test(1, vec![state, team]);
        data.set_tracks_for_test(2, vec![pad]);
        (data, vec![1, 2])
    }

    fn opts<'a>(
        by_depth: bool,
        changes_only: bool,
        filter: &'a NameFilter,
        hidden: &'a BTreeSet<String>,
    ) -> RowOptions<'a> {
        RowOptions { by_depth, changes_only, filter, hidden }
    }

    #[test]
    fn rows_follow_the_levels_of_each_parameter() {
        let (data, entities) = data();
        let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
        let s = rows(&data, &entities, &opts(true, false, &filter, &hidden));
        assert_eq!(s.len(), 2);
        let first: Vec<_> = s[0].rows.iter().map(|r| (r.track, r.level, r.first)).collect();
        assert_eq!(first, [(0, Some(0), true), (0, Some(1), false), (1, Some(0), true)]);
        assert_eq!(s[1].slot, 1);

        let flat = rows(&data, &entities, &opts(false, false, &filter, &hidden));
        assert_eq!(flat[0].rows.len(), 2, "one row per parameter");
        assert!(flat[0].rows.iter().all(|r| r.level.is_none() && r.first));
    }

    #[test]
    fn rows_that_never_change_can_be_left_out() {
        let (data, entities) = data();
        let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
        let s = rows(&data, &entities, &opts(true, true, &filter, &hidden));
        // Game never changes at the top level and Blue never changes, so only "Idle -> Run" is left.
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].rows.len(), 1);
        assert_eq!(s[0].rows[0].level, Some(1));
        assert!(s[0].rows[0].first, "the surviving row carries the parameter's name");
    }

    #[test]
    fn filter_and_hidden_names_remove_parameters_and_empty_entities() {
        let (data, entities) = data();
        let hidden = BTreeSet::from(["Team".to_string()]);
        let none = NameFilter::default();
        let s = rows(&data, &entities, &opts(true, false, &none, &hidden));
        assert_eq!(s[0].rows.len(), 2);

        let filter = NameFilter::new("team");
        let s = rows(&data, &entities, &opts(true, false, &filter, &BTreeSet::new()));
        assert_eq!(s.len(), 1, "entity 2 has nothing that matches");
        assert_eq!(s[0].rows.len(), 1);
    }

    #[test]
    fn events_are_every_start_and_stop_in_order() {
        let (data, entities) = data();
        let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
        let sections = rows(&data, &entities, &opts(true, false, &filter, &hidden));
        let levels: Vec<&Level> =
            sections.iter().flat_map(|s| &s.rows).map(|r| row_level(&data, &entities, r)).collect();
        let events = event_times(levels.iter().copied());
        // A change in the detail after the | (at 8 s) does not move a block boundary.
        assert_eq!(events, [0.0, 4.0, 10.0]);
        assert_eq!(neighbouring_event(&events, 4.0, true), Some(10.0));
        assert_eq!(neighbouring_event(&events, 4.0, false), Some(0.0));
        assert_eq!(neighbouring_event(&events, 5.0, true), Some(10.0));
        assert_eq!(neighbouring_event(&events, 5.0, false), Some(4.0));
        assert_eq!(neighbouring_event(&events, 10.0, true), None);
        assert_eq!(neighbouring_event(&events, 0.0, false), None);
        assert_eq!(neighbouring_event(&[], 1.0, true), None);
    }

    #[test]
    fn the_smallest_block_under_the_playhead() {
        let (data, entities) = data();
        let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
        let sections = rows(&data, &entities, &opts(true, false, &filter, &hidden));
        let levels: Vec<&Level> =
            sections.iter().flat_map(|s| &s.rows).map(|r| row_level(&data, &entities, r)).collect();
        assert_eq!(smallest_block_at(levels.iter().copied(), 5.0), Some((4.0, 10.0)));
        assert_eq!(smallest_block_at(levels.iter().copied(), 1.0), Some((0.0, 4.0)));
        assert_eq!(smallest_block_at(levels.iter().copied(), 12.0), None);
    }

    #[test]
    fn long_shared_prefixes_are_dropped_from_names() {
        let names = [
            "#24579765: imperial_stormtrooper#1.Slot: AvoidDangerCheck.AvoidDangerZoneBehaviorComponent",
            "#24579765: imperial_stormtrooper#1.Slot: BasicAttack.BasicAttackBehaviorComponent",
        ];
        assert_eq!(
            short_names(&names),
            ["AvoidDangerCheck.AvoidDangerZoneBehaviorComponent", "BasicAttack.BasicAttackBehaviorComponent"]
        );
        // Cut at a separator, never in the middle of a word.
        assert_eq!(short_names(&["Physics.QueriesA", "Physics.QueriesB"]), ["QueriesA", "QueriesB"]);
        assert_eq!(short_names(&["CharacterState", "CharacterStyle"]), ["CharacterState", "CharacterStyle"]);
        // Nothing to strip, or nothing would be left.
        assert_eq!(short_names(&["Only"]), ["Only"]);
        // Each name is cut against the sibling it resembles most, so unrelated names are untouched.
        assert_eq!(
            short_names(&["#1: Unit.Slot: Attack.Run", "#1: Unit.Slot: Move.Run", "CharacterState", "ai_intent"]),
            ["Attack.Run", "Move.Run", "CharacterState", "ai_intent"]
        );
        assert_eq!(short_names(&["A.", "A.B"]), ["A.", "A.B"]);
        assert_eq!(short_names(&["Game.X", "Game.Y"]), ["X", "Y"]);
        assert_eq!(short_names(&["G.X", "G.Y"]), ["G.X", "G.Y"], "not worth cutting three characters");
        assert!(short_names(&[]).is_empty());
    }

    #[test]
    fn frame_stripes_appear_only_when_frames_are_wide() {
        let frames: Vec<f32> = (0..=100).map(|i| i as f32 * 0.1).collect();
        // 10 s over 1000 px: a frame is 10 px wide.
        let s = frame_stripes(&frames, 0.0, 10.0, 1000.0, 4.0);
        assert_eq!(s.len(), 50);
        assert_eq!(s[0], (10.0, 20.0));
        // 10 s over 100 px: 1 px frames, no stripes.
        assert!(frame_stripes(&frames, 0.0, 10.0, 100.0, 4.0).is_empty());
        // A window inside the capture only gets what is in it.
        let s = frame_stripes(&frames, 0.95, 1.55, 600.0, 4.0);
        assert!(s.iter().all(|(a, b)| *a >= 0.0 && *b <= 600.0 && b > a));
        assert_eq!(s.len(), 4, "frames 9, 11, 13 and 15, the first and last only partly in view");
        assert_eq!(s[0], (0.0, 50.0));
        assert!(frame_stripes(&[0.0], 0.0, 1.0, 100.0, 1.0).is_empty());
        assert!(frame_stripes(&frames, 5.0, 5.0, 100.0, 1.0).is_empty());
    }
}
