//! Drives the graphs panel with real pointer and keyboard input against the sample capture.

use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, pos2, vec2};
use vrd_ui_egui::{BottomTab, ReplayApp};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

fn frame(ctx: &Context, app: &mut ReplayApp, events: Vec<Event>) {
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1500.0, 1000.0))),
        events,
        ..RawInput::default()
    };
    let _ = ctx.run(input, |ctx| app.ui(ctx));
}

fn id(app: &ReplayApp, name: &str) -> i32 {
    app.replay().unwrap().entities().find(|e| e.name == name).unwrap().id
}

/// The sample, with the graphs tab open on SystemStats.
fn graphs() -> (Context, ReplayApp) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    let stats = id(&app, "SystemStats");
    app.select(Some(stats));
    app.set_bottom_tab(BottomTab::Graphs);
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    (ctx, app)
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

fn key(key: Key) -> Vec<Event> {
    vec![Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: Modifiers::NONE }]
}

/// A point at `fx` across the plotting area and `fy` down it (below the time ruler).
fn at(app: &ReplayApp, fx: f32, fy: f32) -> Pos2 {
    let r = app.graphs().rect();
    assert!(r.width() > 200.0 && r.height() > 80.0, "the graphs are laid out: {r:?}");
    let top = r.top() + 18.0;
    pos2(r.left() + fx * r.width(), top + fy * (r.bottom() - top))
}

fn drag(ctx: &Context, app: &mut ReplayApp, from: Pos2, to: Pos2) {
    frame(ctx, app, vec![Event::PointerMoved(from)]);
    frame(ctx, app, vec![button(from, true)]);
    let steps = 4;
    for i in 1..=steps {
        let p = from + (to - from) * (i as f32 / steps as f32);
        frame(ctx, app, vec![Event::PointerMoved(p)]);
    }
    frame(ctx, app, vec![button(to, false)]);
    frame(ctx, app, vec![]);
}

fn click(ctx: &Context, app: &mut ReplayApp, p: Pos2) {
    frame(ctx, app, vec![Event::PointerMoved(p)]);
    frame(ctx, app, vec![button(p, true)]);
    frame(ctx, app, vec![button(p, false)]);
    frame(ctx, app, vec![]);
}

#[test]
fn the_selected_entity_is_graphed() {
    let (_ctx, app) = graphs();
    assert_eq!(app.graphs().shown(), (1, 5), "SystemStats: DrawCalls, FPS, GPUTime, Triangles, timestepMS");
}

#[test]
fn clicking_the_graph_seeks_there() {
    let (ctx, mut app) = graphs();
    let total = app.replay().unwrap().total_time();
    let p = at(&app, 0.75, 0.5);
    click(&ctx, &mut app, p);
    let t = app.playback().time();
    assert!((t - 0.75 * total).abs() < total * 0.01, "{t} vs {}", 0.75 * total);
    assert!(!app.playback().is_playing());
}

#[test]
fn dragging_the_playhead_scrubs() {
    let (ctx, mut app) = graphs();
    let total = app.replay().unwrap().total_time();
    app.seek_frame(app.replay().unwrap().frame_for_time(total * 0.25));
    frame(&ctx, &mut app, vec![]);
    let x = app.playback().time() / total;
    let (from, to) = (at(&app, x as f32, 0.5), at(&app, 0.6, 0.5));
    drag(&ctx, &mut app, from, to);
    let t = app.playback().time();
    assert!((t - 0.6 * total).abs() < total * 0.02, "scrubbed to {t}");
    assert!(app.time_window().is_full(), "scrubbing does not move the view");
}

#[test]
fn scrolling_zooms_about_the_pointer_and_double_click_fits() {
    let (ctx, mut app) = graphs();
    let p = at(&app, 0.25, 0.5);
    let t_under = app.time_window().time_at(0.25);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
    for _ in 0..4 {
        frame(
            &ctx,
            &mut app,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 60.0),
                modifiers: Modifiers::NONE,
            }],
        );
    }
    for _ in 0..10 {
        frame(&ctx, &mut app, vec![]);
    }
    let w = app.time_window();
    assert!(w.range() < w.total() * 0.9, "zoomed in: {}..{}", w.start(), w.end());
    assert!((w.time_at(0.25) - t_under).abs() < w.range() * 0.02, "the time under the pointer stayed put");

    let p = at(&app, 0.5, 0.5);
    frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![button(p, true)]);
        frame(&ctx, &mut app, vec![button(p, false)]);
    }
    frame(&ctx, &mut app, vec![]);
    assert!(app.time_window().is_full(), "double-click fits the whole capture");
}

#[test]
fn dragging_elsewhere_pans_the_view() {
    let (ctx, mut app) = graphs();
    let total = app.replay().unwrap().total_time();
    app.time_window_mut().set(total * 0.4, total * 0.5);
    let playhead = app.playback().time();
    let before = app.time_window().start();
    // Drag right: earlier times come into view.
    let (from, to) = (at(&app, 0.3, 0.5), at(&app, 0.7, 0.5));
    drag(&ctx, &mut app, from, to);
    let after = app.time_window().start();
    assert!(after < before - total * 0.03, "{before} -> {after}");
    assert_eq!(app.playback().time(), playhead, "panning does not seek");
}

#[test]
fn the_view_follows_playback_and_can_be_left_alone() {
    let (ctx, mut app) = graphs();
    let total = app.replay().unwrap().total_time();
    app.time_window_mut().set(0.0, total * 0.1);
    let replay_frame = app.replay().unwrap().frame_for_time(total * 0.5);
    app.seek_frame(replay_frame);
    frame(&ctx, &mut app, vec![]);
    let w = app.time_window();
    let t = app.playback().time();
    assert!(w.start() <= t && t <= w.end(), "a jump brings the playhead into view");

    app.graphs_mut().follow = false;
    app.seek_frame(app.replay().unwrap().frame_for_time(total * 0.05));
    frame(&ctx, &mut app, vec![]);
    let w = app.time_window();
    assert!(app.playback().time() < w.start(), "with follow off the view stays put");
}

#[test]
fn pinning_keeps_an_entity_graphed_across_selections() {
    let (ctx, mut app) = graphs();
    let stats = id(&app, "SystemStats");
    let player = id(&app, "PlayerCharacter");
    let navmesh = id(&app, "NavMesh");

    app.graphs_mut().toggle_pin(stats);
    app.select(Some(player));
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (2, 7), "pinned stats plus the selected player");

    // Selecting something without numbers leaves just the pinned one.
    app.select(Some(navmesh));
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (1, 5));

    app.graphs_mut().toggle_pin(stats);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (0, 0));
}

#[test]
fn the_filter_limits_which_series_are_drawn() {
    let (ctx, mut app) = graphs();
    app.graphs_mut().filter = "fps|gpu".into();
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (1, 2));
    app.graphs_mut().filter = "nothing matches this".into();
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.graphs().shown(), (0, 0));
}

#[test]
fn overlaying_entities_shares_bands() {
    let (ctx, mut app) = graphs();
    let troopers: Vec<i32> =
        ["imperial_stormtrooper#1", "imperial_stormtrooper#4"].iter().map(|n| id(&app, n)).collect();
    for t in &troopers {
        app.graphs_mut().toggle_pin(*t);
    }
    app.select(Some(id(&app, "PlayerCharacter")));
    app.graphs_mut().stacking.by_entity = false;
    frame(&ctx, &mut app, vec![]);
    // health from three entities, speed from the player.
    assert_eq!(app.graphs().shown(), (3, 4));
}

#[test]
fn g_and_l_switch_tabs() {
    let (ctx, mut app) = graphs();
    frame(&ctx, &mut app, key(Key::L));
    assert_eq!(app.bottom_tab(), BottomTab::Logs);
    frame(&ctx, &mut app, key(Key::G));
    assert_eq!(app.bottom_tab(), BottomTab::Graphs);
}

#[test]
fn hovering_and_playing_over_every_entity_does_not_panic() {
    let (ctx, mut app) = graphs();
    let ids: Vec<i32> = app.replay().unwrap().entities().map(|e| e.id).collect();
    for (k, id) in ids.iter().enumerate() {
        app.select(Some(*id));
        app.graphs_mut().stacking.by_entity = k % 2 == 0;
        app.graphs_mut().ignore_outliers = k % 3 == 0;
        frame(&ctx, &mut app, vec![]);
        // Entities without numbers show a hint instead of graphs.
        if app.graphs().rect().is_positive() {
            let p = at(&app, 0.1 + 0.8 * (k as f32 / ids.len() as f32), 0.4);
            frame(&ctx, &mut app, vec![Event::PointerMoved(p)]);
        }
    }
    frame(&ctx, &mut app, key(Key::Space));
    for _ in 0..30 {
        frame(&ctx, &mut app, vec![]);
    }
}
