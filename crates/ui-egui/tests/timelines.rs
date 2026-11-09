//! Drives the parameter timelines with real pointer and keyboard input against the sample capture.

use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, pos2, vec2};
use vrd_graph::TimelineData;
use vrd_ui_egui::{BottomTab, HEADER_HEIGHT, ROW_HEIGHT, ReplayApp};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

thread_local! {
    /// A simulated clock: egui times double-clicks by it, and wall-clock time in a debug build is too slow.
    static CLOCK: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn frame(ctx: &Context, app: &mut ReplayApp, events: Vec<Event>) {
    let time = CLOCK.with(|c| {
        c.set(c.get() + 0.016);
        c.get()
    });
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1500.0, 1000.0))),
        time: Some(time),
        events,
        ..RawInput::default()
    };
    let _ = ctx.run(input, |ctx| app.ui(ctx));
}

fn id(app: &ReplayApp, name: &str) -> i32 {
    app.replay().unwrap().entities().find(|e| e.name == name).unwrap().id
}

/// The sample, with the timelines tab open on the player.
fn timelines() -> (Context, ReplayApp) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    let player = id(&app, "PlayerCharacter");
    app.select(Some(player));
    app.set_bottom_tab(BottomTab::Timelines);
    // The properties panel grows to fit the widest value it ever shows and never shrinks, which moves
    // the timelines sideways. Visit the whole capture first so that the layout is final.
    let last = app.replay().unwrap().last_frame();
    for f in (0..=last).step_by(25) {
        app.seek_frame(f);
        frame(&ctx, &mut app, vec![]);
    }
    app.seek_frame(0);
    settle(&ctx, &mut app);
    (ctx, app)
}

/// Runs frames until the layout stops moving: panels size themselves to their content over a few frames.
fn settle(ctx: &Context, app: &mut ReplayApp) {
    let mut last = egui::Rect::NOTHING;
    let mut stable = 0;
    for _ in 0..30 {
        frame(ctx, app, vec![]);
        let now = app.timelines().plot_rect();
        stable = if now == last { stable + 1 } else { 0 };
        last = now;
        if stable >= 3 {
            // Long enough for egui not to take the next click for part of a multi-click.
            CLOCK.with(|c| c.set(c.get() + 0.5));
            return;
        }
    }
    panic!("the layout never settled: {last:?}");
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

fn key(key: Key) -> Vec<Event> {
    vec![Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: Modifiers::NONE }]
}

/// A point at time `t` along a row, `row` rows below the first entity header.
fn at(app: &ReplayApp, t: f64, row: usize) -> Pos2 {
    let plot = app.timelines().plot_rect();
    assert!(plot.width() > 200.0 && plot.height() > 40.0, "the timelines are laid out: {plot:?}");
    let w = app.time_window();
    let x = plot.left() + w.fraction_of(t) as f32 * plot.width();
    pos2(x, plot.top() + HEADER_HEIGHT + (row as f32 + 0.5) * ROW_HEIGHT)
}

fn click(ctx: &Context, app: &mut ReplayApp, p: Pos2) {
    frame(ctx, app, vec![Event::PointerMoved(p)]);
    frame(ctx, app, vec![button(p, true)]);
    frame(ctx, app, vec![button(p, false)]);
    frame(ctx, app, vec![]);
}

fn drag(ctx: &Context, app: &mut ReplayApp, from: Pos2, to: Pos2) {
    frame(ctx, app, vec![Event::PointerMoved(from)]);
    frame(ctx, app, vec![button(from, true)]);
    for i in 1..=4 {
        frame(ctx, app, vec![Event::PointerMoved(from + (to - from) * (i as f32 / 4.0))]);
    }
    frame(ctx, app, vec![button(to, false)]);
    frame(ctx, app, vec![]);
}

#[test]
fn the_player_shows_a_row_per_level_and_the_flat_view_one_per_parameter() {
    let (ctx, mut app) = timelines();
    assert_eq!(app.timelines().shown(), (1, 7), "CharacterState x3 levels, MoveState x4");
    app.timelines_mut().by_depth = false;
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown(), (1, 2));
    app.timelines_mut().by_depth = true;
    app.timelines_mut().changes_only = true;
    frame(&ctx, &mut app, vec![]);
    let (_, rows) = app.timelines().shown();
    assert!(rows > 0 && rows < 7, "{rows} rows change");
}

#[test]
fn clicking_a_block_seeks_to_that_time() {
    let (ctx, mut app) = timelines();
    let total = app.replay().unwrap().total_time();
    let p = at(&app, total * 0.6, 2);
    click(&ctx, &mut app, p);
    let t = app.playback().time();
    assert!((t - total * 0.6).abs() < total * 0.01, "{t}");
}

#[test]
fn scrolling_over_the_blocks_zooms_about_the_pointer_but_over_the_labels_it_does_not() {
    let (ctx, mut app) = timelines();
    let total = app.replay().unwrap().total_time();
    let wheel =
        || Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, 60.0), modifiers: Modifiers::NONE };

    let labels = pos2(app.timelines().plot_rect().left() - 100.0, app.timelines().plot_rect().top() + 40.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(labels)]);
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![wheel()]);
    }
    assert!(app.time_window().is_full(), "the wheel over the labels scrolls rows, not time");

    let t = total * 0.3;
    let p = at(&app, t, 1);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
    let fraction = app.time_window().fraction_of(t);
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![wheel()]);
    }
    for _ in 0..8 {
        frame(&ctx, &mut app, vec![]);
    }
    let w = app.time_window();
    assert!(w.range() < total * 0.9, "zoomed: {}..{}", w.start(), w.end());
    assert!((w.fraction_of(t) - fraction).abs() < 0.02, "{t}s stayed under the pointer");
}

#[test]
fn double_clicking_a_block_zooms_to_it_and_empty_space_fits_everything() {
    let (ctx, mut app) = timelines();
    let player = id(&app, "PlayerCharacter");
    let data = TimelineData::build(app.replay().unwrap());
    let track = data.tracks(player).iter().find(|t| t.name == "CharacterState").unwrap();
    // The longest stretch of one state that is neither the first nor the last: easy to hit.
    let run = track.levels[2].runs[1..track.levels[2].runs.len() - 1]
        .iter()
        .filter(|r| r.key != 0)
        .max_by(|a, b| (a.end - a.start).total_cmp(&(b.end - b.start)))
        .copied()
        .unwrap();
    let middle = (run.start + run.end) / 2.0;
    let p = at(&app, middle, 2);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![button(p, true)]);
        frame(&ctx, &mut app, vec![button(p, false)]);
    }
    frame(&ctx, &mut app, vec![]);
    let w = app.time_window();
    assert!(
        w.start() <= run.start && w.end() >= run.end,
        "{}..{} covers {}..{}",
        w.start(),
        w.end(),
        run.start,
        run.end
    );
    assert!(w.range() < (run.end - run.start) * 1.5, "and not much more: {}", w.range());

    // Below the last row there is nothing to zoom to, so a double-click shows everything again.
    app.timelines_mut().filter = "movestate".into();
    settle(&ctx, &mut app);
    let w = app.time_window();
    let p = at(&app, w.start() + w.range() * 0.5, 6);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![button(p, true)]);
        frame(&ctx, &mut app, vec![button(p, false)]);
    }
    frame(&ctx, &mut app, vec![]);
    assert!(app.time_window().is_full());
}

#[test]
fn dragging_pans_and_dragging_the_playhead_scrubs() {
    let (ctx, mut app) = timelines();
    let total = app.replay().unwrap().total_time();
    app.time_window_mut().set(total * 0.4, total * 0.5);
    let playhead = app.playback().time();
    let (from, to) = (at(&app, total * 0.42, 3), at(&app, total * 0.48, 3));
    let before = app.time_window().start();
    drag(&ctx, &mut app, from, to);
    assert!(app.time_window().start() < before, "dragging right brings earlier times into view");
    assert_eq!(app.playback().time(), playhead, "panning does not seek");

    app.time_window_mut().fill();
    app.seek_frame(app.replay().unwrap().frame_for_time(total * 0.25));
    frame(&ctx, &mut app, vec![]);
    let t = app.playback().time();
    let (from, to) = (at(&app, t, 3), at(&app, total * 0.7, 3));
    drag(&ctx, &mut app, from, to);
    assert!((app.playback().time() - total * 0.7).abs() < total * 0.02);
    assert!(app.time_window().is_full());
}

#[test]
fn event_buttons_step_the_playhead_between_changes() {
    let (_ctx, mut app) = timelines();
    app.seek_frame(0);
    // The first change is not at 0, so start there: there is nothing before it to step back to.
    app.step_timeline_event(true);
    let mut times = vec![app.playback().time()];
    assert!(times[0] > 0.0);
    for _ in 0..5 {
        app.step_timeline_event(true);
        times.push(app.playback().time());
    }
    assert!(times.windows(2).all(|w| w[1] > w[0]), "each step goes forward: {times:?}");
    for expected in times.iter().rev().skip(1) {
        app.step_timeline_event(false);
        assert_eq!(app.playback().time(), *expected, "and back again");
    }
    app.step_timeline_event(false);
    assert_eq!(app.playback().time(), times[0], "nothing before the first change");
    app.seek_frame(app.replay().unwrap().last_frame());
    let end = app.playback().time();
    app.step_timeline_event(true);
    assert_eq!(app.playback().time(), end, "nothing after the last change");
}

#[test]
fn pinning_keeps_an_entity_across_selections_in_both_panels() {
    let (ctx, mut app) = timelines();
    let player = id(&app, "PlayerCharacter");
    let trooper = id(&app, "imperial_stormtrooper#1");
    let navmesh = id(&app, "NavMesh");

    app.toggle_pin(player);
    app.select(Some(trooper));
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown().0, 2);

    app.select(Some(navmesh));
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown(), (1, 7), "NavMesh has no text parameters, the pinned player stays");

    app.set_bottom_tab(BottomTab::Graphs);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (1, 2), "the player has numbers too, so the same pin shows in the graphs");
    assert!(app.timelines().pins().contains(player));

    app.toggle_pin(player);
    app.set_bottom_tab(BottomTab::Timelines);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown(), (0, 0));
}

#[test]
fn the_filter_narrows_the_rows() {
    let (ctx, mut app) = timelines();
    app.timelines_mut().filter = "movestate".into();
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown(), (1, 4));
    app.timelines_mut().filter = "no such parameter".into();
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.timelines().shown(), (0, 0));
}

#[test]
fn t_opens_the_tab_and_other_keys_still_work() {
    let (ctx, mut app) = timelines();
    app.set_bottom_tab(BottomTab::Logs);
    frame(&ctx, &mut app, key(Key::T));
    assert_eq!(app.bottom_tab(), BottomTab::Timelines);
    frame(&ctx, &mut app, key(Key::G));
    assert_eq!(app.bottom_tab(), BottomTab::Graphs);
    frame(&ctx, &mut app, key(Key::L));
    assert_eq!(app.bottom_tab(), BottomTab::Logs);
}

#[test]
fn every_entity_and_mode_can_be_drawn_and_hovered_while_playing() {
    let (ctx, mut app) = timelines();
    let ids: Vec<i32> = app.replay().unwrap().entities().map(|e| e.id).collect();
    for (k, entity) in ids.iter().enumerate() {
        app.select(Some(*entity));
        app.timelines_mut().by_depth = k % 2 == 0;
        app.timelines_mut().changes_only = k % 3 == 0;
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        let plot = app.timelines().plot_rect();
        if plot.is_positive() {
            let p = pos2(plot.left() + plot.width() * (0.1 + 0.8 * k as f32 / ids.len() as f32), plot.top() + 40.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
        }
    }
    let trooper = id(&app, "imperial_stormtrooper#1");
    app.select(Some(trooper));
    app.time_window_mut().fill();
    frame(&ctx, &mut app, key(Key::Space));
    for _ in 0..30 {
        frame(&ctx, &mut app, vec![]);
    }
}
