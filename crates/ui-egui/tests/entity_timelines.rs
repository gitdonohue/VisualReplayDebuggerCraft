//! Drives the entity timelines with real pointer input against the sample capture.

use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, pos2, vec2};
use vrd_ui_egui::{BottomTab, ENTITY_ROW_HEIGHT, ReplayApp};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

thread_local! {
    static CLOCK: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn frame(ctx: &Context, app: &mut ReplayApp, events: Vec<Event>) {
    let time = CLOCK.with(|c| {
        c.set(c.get() + 0.6);
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

fn open() -> (Context, ReplayApp) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    app.set_bottom_tab(BottomTab::EntityTimelines);
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![]);
    }
    (ctx, app)
}

fn click(ctx: &Context, app: &mut ReplayApp, at: Pos2) {
    let button =
        |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    frame(ctx, app, vec![Event::PointerMoved(at)]);
    frame(ctx, app, vec![button(true)]);
    frame(ctx, app, vec![button(false)]);
    frame(ctx, app, vec![]);
}

#[test]
fn every_entity_gets_a_row() {
    let (_, app) = open();
    assert_eq!(app.bottom_tab(), BottomTab::EntityTimelines);
    assert!(app.entity_timelines().plot_rect().is_positive());
    assert_eq!(app.entity_timelines().shown(), app.replay().unwrap().entity_count());
}

#[test]
fn clicking_a_name_selects_and_the_eye_hides_it_in_3d() {
    let (ctx, mut app) = open();
    let plot = app.entity_timelines().plot_rect();
    let row = 3;
    let y = plot.top() + (row as f32 + 0.5) * ENTITY_ROW_HEIGHT;

    click(&ctx, &mut app, pos2(plot.left() - 100.0, y));
    let selected = app.selected().expect("a row was clicked");
    assert!(!app.view_options().hidden_entities.contains(&selected));

    click(&ctx, &mut app, pos2(plot.left() - 232.0, y));
    assert!(app.view_options().hidden_entities.contains(&selected), "the eye hides the entity");
    click(&ctx, &mut app, pos2(plot.left() - 232.0, y));
    assert!(!app.view_options().hidden_entities.contains(&selected));
}

#[test]
fn clicking_the_bars_seeks_and_the_filter_narrows_the_rows() {
    let (ctx, mut app) = open();
    let plot = app.entity_timelines().plot_rect();
    click(&ctx, &mut app, pos2(plot.center().x, plot.top() + 10.0));
    let t = app.playback().time();
    assert!((t - app.replay().unwrap().total_time() / 2.0).abs() < 2.0, "seeked to about the middle, got {t}");

    app.entity_timelines_mut().filter = "stormtrooper".into();
    frame(&ctx, &mut app, vec![]);
    let n = app.entity_timelines().shown();
    assert!(n > 0 && n < app.replay().unwrap().entity_count());

    app.entity_timelines_mut().filter = String::new();
    app.entity_timelines_mut().logged_only = true;
    frame(&ctx, &mut app, vec![]);
    assert!(app.entity_timelines().shown() < app.replay().unwrap().entity_count());
}
