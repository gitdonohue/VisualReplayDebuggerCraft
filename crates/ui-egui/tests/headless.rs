//! Runs the whole UI headlessly against the real sample capture.

use egui::{Context, Event, Key, Modifiers, RawInput};
use vrd_ui_egui::ReplayApp;

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

fn frame(ctx: &Context, app: &mut ReplayApp, events: Vec<Event>) {
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 800.0))),
        events,
        ..RawInput::default()
    };
    let _ = ctx.run(input, |ctx| app.ui(ctx));
}

fn key(key: Key) -> Vec<Event> {
    vec![Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: Modifiers::NONE }]
}

#[test]
fn renders_the_welcome_screen_without_a_capture() {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    assert!(app.replay().is_none());
}

#[test]
fn renders_a_real_capture_and_responds_to_keys() {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    assert!(app.replay().is_some());

    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.playback().frame(), 0);

    frame(&ctx, &mut app, key(Key::ArrowRight));
    assert_eq!(app.playback().frame(), 1);
    frame(&ctx, &mut app, key(Key::ArrowRight));
    frame(&ctx, &mut app, key(Key::ArrowLeft));
    assert_eq!(app.playback().frame(), 1);

    frame(&ctx, &mut app, key(Key::End));
    let last = app.replay().unwrap().last_frame();
    assert_eq!(app.playback().frame(), last);
    frame(&ctx, &mut app, key(Key::Home));
    assert_eq!(app.playback().frame(), 0);

    frame(&ctx, &mut app, key(Key::Space));
    assert!(app.playback().is_playing());
    frame(&ctx, &mut app, key(Key::Space));
    assert!(!app.playback().is_playing());

    frame(&ctx, &mut app, key(Key::PageDown));
    assert!(app.playback().frame() > 0, "jumped to a frame with logs");
}

#[test]
fn every_entity_can_be_selected_and_drawn_at_any_frame() {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    let ids: Vec<i32> = app.replay().unwrap().entities().map(|e| e.id).collect();
    let last = app.replay().unwrap().last_frame();

    for (n, id) in ids.iter().enumerate() {
        app.select(Some(*id));
        app.seek_frame((n as i32 * 37) % (last + 1));
        frame(&ctx, &mut app, vec![]);
    }
}

#[test]
fn a_bad_file_reports_an_error_and_keeps_the_current_capture() {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    app.open_bytes("junk.vrd", b"definitely not a capture");
    assert!(app.replay().is_some(), "the previous capture stays open");
    frame(&ctx, &mut app, vec![]);

    let mut fresh = ReplayApp::new();
    fresh.open_bytes("empty.vrd", &[]);
    assert!(fresh.replay().is_none());
    frame(&ctx, &mut fresh, vec![]);
}

fn loaded() -> (Context, ReplayApp) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
    }
    (ctx, app)
}

fn centre(app: &ReplayApp) -> egui::Pos2 {
    let r = app.viewport_rect();
    assert!(r.width() > 100.0 && r.height() > 100.0, "the viewport is laid out: {r:?}");
    r.center()
}

fn button(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

#[test]
fn the_viewport_follows_the_main_camera_until_the_user_moves_it() {
    let (ctx, mut app) = loaded();
    assert!(app.view_options().follow_camera, "the sample has a maincamera, so it is followed");
    let start = app.viewport_camera().clone();

    let from = centre(&app);
    let to = from + egui::vec2(60.0, 25.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(from)]);
    frame(&ctx, &mut app, vec![button(from, true)]);
    frame(&ctx, &mut app, vec![Event::PointerMoved(from + egui::vec2(30.0, 10.0))]);
    frame(&ctx, &mut app, vec![Event::PointerMoved(to)]);
    frame(&ctx, &mut app, vec![button(to, false)]);

    assert!(!app.view_options().follow_camera, "dragging takes over from the followed camera");
    let after = app.viewport_camera();
    assert!((after.yaw - start.yaw).abs() > 0.1, "orbited: {} -> {}", start.yaw, after.yaw);

    // It stays where the user put it as frames advance.
    let held = after.clone();
    app.seek_frame(500);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.viewport_camera().yaw, held.yaw);
}

#[test]
fn scrolling_over_the_viewport_zooms() {
    let (ctx, mut app) = loaded();
    let at = centre(&app);
    frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
    let before = app.viewport_camera().distance;
    for _ in 0..3 {
        frame(
            &ctx,
            &mut app,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 40.0),
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert!(app.viewport_camera().distance < before, "{} -> {}", before, app.viewport_camera().distance);
}

#[test]
fn clicking_an_entity_in_the_viewport_selects_it() {
    let (ctx, mut app) = loaded();
    let player = app.replay().unwrap().entities().find(|e| e.name == "PlayerCharacter").unwrap().id;
    let other = app.replay().unwrap().entities().find(|e| e.name == "imperial_stormtrooper#1").unwrap().id;

    app.seek_frame(600);
    app.focus_entity(player);
    app.select(Some(other));
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    assert!(!app.view_options().follow_camera, "focusing leaves follow mode");

    let at = centre(&app);
    frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
    frame(&ctx, &mut app, vec![button(at, true)]);
    frame(&ctx, &mut app, vec![button(at, false)]);
    assert_eq!(app.selected(), Some(player), "the focused entity is at the centre of the view");

    // Clicking empty space keeps the current selection.
    let empty = app.viewport_rect().left_top() + egui::vec2(20.0, 60.0);
    frame(&ctx, &mut app, vec![Event::PointerMoved(empty)]);
    frame(&ctx, &mut app, vec![button(empty, true)]);
    frame(&ctx, &mut app, vec![button(empty, false)]);
    assert_eq!(app.selected(), Some(player));
}

#[test]
fn without_a_gpu_the_viewport_still_runs_and_says_so() {
    let (_ctx, app) = loaded();
    assert!(!app.has_3d());
}

#[test]
fn the_sample_is_detected_as_y_up() {
    let (_ctx, app) = loaded();
    assert_eq!(app.viewport_camera().up_axis, vrd_scene::Axis::Y);
}
