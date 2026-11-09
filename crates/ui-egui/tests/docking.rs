//! The dockable panels: several graphs and timelines views, each drawn on its own.

use egui::{Context, Event, RawInput, Rect, vec2};
use vrd_ui_egui::{BottomTab, ReplayApp};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

fn frames(ctx: &Context, app: &mut ReplayApp, count: usize) {
    for i in 0..count {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1500.0, 1000.0))),
            time: Some(i as f64 * 0.016),
            events: Vec::<Event>::new(),
            ..RawInput::default()
        };
        let _ = ctx.run(input, |ctx| app.ui(ctx));
    }
}

fn open() -> (Context, ReplayApp) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    app.open_bytes("sample.vrd", &sample());
    let stats = app.replay().unwrap().entities().find(|e| e.name == "SystemStats").unwrap().id;
    app.select(Some(stats));
    frames(&ctx, &mut app, 4);
    (ctx, app)
}

#[test]
fn extra_views_are_separate_panels_with_their_own_state() {
    let (ctx, mut app) = open();
    assert_eq!((app.graph_view_count(), app.timeline_view_count()), (1, 1));

    app.set_bottom_tab(BottomTab::Graphs);
    frames(&ctx, &mut app, 3);
    let first = app.graphs().rect();
    assert!(first.is_positive(), "the first graphs view is drawn");

    let second = app.add_graphs_view();
    assert_eq!(app.graph_view_count(), 2);
    frames(&ctx, &mut app, 3);
    assert_eq!(app.bottom_tab(), BottomTab::Graphs);
    assert!(app.graphs().rect().is_positive(), "the new view is drawn");
    assert_ne!(second, 0);

    app.add_timelines_view();
    assert_eq!(app.timeline_view_count(), 2);
    frames(&ctx, &mut app, 3);
    assert_eq!(app.bottom_tab(), BottomTab::Timelines);
}

#[test]
fn layout_can_be_reset_and_a_new_capture_drops_extra_views() {
    let (ctx, mut app) = open();
    app.add_graphs_view();
    app.add_timelines_view();
    frames(&ctx, &mut app, 3);

    app.reset_layout();
    assert_eq!((app.graph_view_count(), app.timeline_view_count()), (1, 1));
    frames(&ctx, &mut app, 3);
    assert!(app.viewport_rect().is_positive());

    app.add_graphs_view();
    app.open_bytes("sample.vrd", &sample());
    assert_eq!(app.graph_view_count(), 1);
    frames(&ctx, &mut app, 3);
}

#[test]
fn closed_panels_come_back_when_asked_for() {
    let (ctx, mut app) = open();
    app.set_bottom_tab(BottomTab::Logs);
    frames(&ctx, &mut app, 3);
    assert_eq!(app.bottom_tab(), BottomTab::Logs);
    app.set_bottom_tab(BottomTab::Timelines);
    frames(&ctx, &mut app, 3);
    assert_eq!(app.bottom_tab(), BottomTab::Timelines);
}

#[test]
fn every_view_keeps_its_own_pins() {
    let (ctx, mut app) = open();
    let stats = app.selected().unwrap();
    app.set_bottom_tab(BottomTab::Graphs);
    frames(&ctx, &mut app, 3);
    app.graphs_mut().pins_mut().toggle(stats);
    assert!(app.graphs().pins().contains(stats));

    app.add_graphs_view();
    frames(&ctx, &mut app, 3);
    assert!(!app.graphs().pins().contains(stats), "a new view starts with nothing pinned");
    assert!(app.graph_view_count() == 2);
}

#[test]
fn a_panel_can_live_in_a_window_of_its_own() {
    let (ctx, mut app) = open();
    app.set_bottom_tab(BottomTab::Graphs);
    frames(&ctx, &mut app, 3);
    app.detach(BottomTab::Graphs);
    assert_eq!(app.detached_count(), 1);
    frames(&ctx, &mut app, 3);
    assert!(app.graphs().rect().is_positive(), "the detached panel is still drawn");

    app.set_bottom_tab(BottomTab::Graphs);
    frames(&ctx, &mut app, 3);
    assert_eq!(app.detached_count(), 1, "asking for it does not dock it again");

    app.reset_layout();
    assert_eq!(app.detached_count(), 0);
    frames(&ctx, &mut app, 3);
}
