use std::collections::BTreeSet;

use vrd_graph::{
    BlockKind, NameFilter, RowOptions, TimelineData, blocks, event_times, row_level, rows, short_names,
    smallest_block_at,
};
use vrd_model::Replay;

fn sample() -> Replay {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    Replay::from_bytes(&bytes).unwrap()
}

fn id(r: &Replay, name: &str) -> i32 {
    r.entities().find(|e| e.name == name).unwrap().id
}

#[test]
fn the_player_has_a_row_per_level_of_each_state_path() {
    let r = sample();
    let data = TimelineData::build(&r);
    let player = id(&r, "PlayerCharacter");
    let tracks = data.tracks(player);
    let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["CharacterState", "MoveState"]);
    assert_eq!(tracks[0].levels.len(), 3, "Game.Character.AttackState");
    assert_eq!(tracks[1].levels.len(), 4, "Game.Movement.States.IdleMoveState");

    let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
    let opts = |by_depth, changes_only| RowOptions { by_depth, changes_only, filter: &filter, hidden: &hidden };
    let sections = rows(&data, &[player], &opts(true, false));
    assert_eq!(sections[0].rows.len(), 7);
    assert_eq!(rows(&data, &[player], &opts(false, false))[0].rows.len(), 2);
    // "Game" never changes, nor does "Character" or "Movement" or "States": only the leaf level moves.
    let changing = rows(&data, &[player], &opts(true, true));
    assert!(changing[0].rows.len() < 7 && !changing[0].rows.is_empty(), "{} rows", changing[0].rows.len());
}

#[test]
fn parameters_that_never_hold_a_value_are_dropped() {
    let r = sample();
    let data = TimelineData::build(&r);
    // The AI controller entities only ever clear their parameters.
    let controller = id(&r, "/Spawns/imperial_stormtrooper#1:CharacterAIController");
    assert!(!data.has_tracks(controller));
    assert!(data.entities().count() < r.entities().filter(|e| !e.params.is_empty()).count());
    assert!(data.has_tracks(id(&r, "imperial_stormtrooper#1")));
}

#[test]
fn runs_are_contiguous_ordered_and_inside_the_lifetime() {
    let r = sample();
    let data = TimelineData::build(&r);
    for entity in data.entities() {
        for track in data.tracks(entity) {
            for level in std::iter::once(&track.whole).chain(&track.levels) {
                for run in &level.runs {
                    assert!(run.start <= run.end, "{}", track.name);
                    assert!(run.start >= track.valid.0 && run.end <= track.valid.1, "{}", track.name);
                    assert!((run.key as usize) < level.labels.len());
                }
                for pair in level.runs.windows(2) {
                    assert_eq!(pair[0].end, pair[1].start, "{}: runs are contiguous", track.name);
                    assert_ne!(pair[0].key, pair[1].key, "{}: neighbouring equal values are merged", track.name);
                }
            }
        }
    }
}

#[test]
fn a_busy_track_is_busy_zoomed_out_and_made_of_blocks_zoomed_in() {
    let r = sample();
    let data = TimelineData::build(&r);
    let trooper = id(&r, "imperial_stormtrooper#1");
    let track = data.tracks(trooper).iter().max_by_key(|t| t.whole.runs.len()).unwrap();
    assert!(track.whole.runs.len() > 500, "{} runs in {}", track.whole.runs.len(), track.name);

    let (t0, t1) = (0.0, r.total_time());
    let overview = blocks(&track.whole.runs, t0, t1, 1500.0);
    assert!(overview.iter().any(|b| matches!(b.kind, BlockKind::Busy { changes, .. } if changes > 20)), "{overview:?}");
    assert!(overview.len() < 150, "the overview stays small: {} blocks", overview.len());

    // Zoom into a stretch of a couple of seconds in the middle of its life.
    let mid = (track.valid.0 + track.valid.1) / 2.0;
    let zoomed = blocks(&track.whole.runs, mid, mid + 1.0, 1500.0);
    assert!(zoomed.iter().any(|b| matches!(b.kind, BlockKind::Value { .. })));
    assert!(zoomed.windows(2).all(|w| w[0].x0 <= w[1].x0 && w[0].x1 <= w[1].x1 + 1e-3), "blocks are in order");
}

#[test]
fn trooper_parameter_names_lose_their_shared_prefix() {
    let r = sample();
    let data = TimelineData::build(&r);
    let tracks = data.tracks(id(&r, "imperial_stormtrooper#1"));
    let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
    let short: Vec<&str> = tracks.iter().map(|t| t.short_name.as_str()).collect();
    assert!(names.iter().zip(&short).all(|(n, s)| n.ends_with(s) && !s.is_empty()));
    assert!(short.iter().all(|s| !s.starts_with('#')), "{short:?}");
    assert!(short.contains(&"CharacterState"), "unrelated names are left alone: {short:?}");
    assert!(short.iter().any(|s| s.starts_with("AvoidDangerCheck")), "{short:?}");
    assert!(short.iter().map(|s| s.len()).max() < names.iter().map(|n| n.len()).max());
    assert_eq!(short_names(&names).iter().map(String::as_str).collect::<Vec<_>>(), short);
}

#[test]
fn events_and_blocks_can_be_navigated() {
    let r = sample();
    let data = TimelineData::build(&r);
    let player = id(&r, "PlayerCharacter");
    let (filter, hidden) = (NameFilter::default(), BTreeSet::new());
    let sections =
        rows(&data, &[player], &RowOptions { by_depth: true, changes_only: false, filter: &filter, hidden: &hidden });
    let levels: Vec<_> = sections[0].rows.iter().map(|row| row_level(&data, &[player], row)).collect();
    let events = event_times(levels.iter().copied());
    assert!(events.len() > 10 && events.windows(2).all(|w| w[0] < w[1]));
    let t = r.total_time() / 2.0;
    let (start, end) = smallest_block_at(levels.iter().copied(), t).expect("something is happening mid-capture");
    assert!(start <= t && t < end);
}

#[test]
fn colours_follow_the_last_level_of_a_path() {
    let r = sample();
    let data = TimelineData::build(&r);
    assert_eq!(data.color_index("Game.Character.AttackState"), data.color_index("AttackState"));
    assert_ne!(data.color_index("AttackState"), data.color_index("IdleState"));
    let again = TimelineData::build(&r);
    assert_eq!(data.color_index("IdleState"), again.color_index("IdleState"), "stable between loads");
}
