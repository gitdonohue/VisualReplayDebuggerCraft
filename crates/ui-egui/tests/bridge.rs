//! The tools agents use on the running app (WebMCP in the browser), driven through the Bridge.

use egui::{Context, RawInput, Rect, vec2};
use serde_json::{Value, json};
use vrd_ui_egui::{BottomTab, Bridge, ReplayApp};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap()
}

fn frames(ctx: &Context, app: &mut ReplayApp, count: usize) {
    for i in 0..count {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1500.0, 1000.0))),
            time: Some(i as f64 * 0.016),
            ..RawInput::default()
        };
        let _ = ctx.run(input, |ctx| app.ui(ctx));
    }
}

fn open() -> (Context, ReplayApp, Bridge) {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    let bridge = app.bridge();
    frames(&ctx, &mut app, 2);
    app.open_bytes("sample.vrd", &sample());
    frames(&ctx, &mut app, 3);
    (ctx, app, bridge)
}

fn state(bridge: &Bridge) -> Value {
    bridge.call("get_view_state", &json!({})).unwrap()
}

#[test]
fn the_tools_are_well_formed_and_need_no_path() {
    let tools = Bridge::definitions();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let unique: std::collections::BTreeSet<&&str> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "names are unique");
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        assert!(name.len() <= 128 && name.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)));
        assert!(!tool["description"].as_str().unwrap().is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(tool["inputSchema"]["properties"].get("path").is_none(), "{name} should not ask for a path");
        assert!(tool["annotations"]["readOnlyHint"].is_boolean());
    }
    for expected in ["capture_info", "search_logs", "get_series", "seek", "select_entity", "show_panel"] {
        assert!(names.contains(&expected), "{expected}");
    }
}

#[test]
fn without_a_capture_tools_say_so() {
    let ctx = Context::default();
    let mut app = ReplayApp::new();
    let bridge = app.bridge();
    frames(&ctx, &mut app, 2);
    assert_eq!(state(&bridge)["capture"], Value::Null);
    let err = bridge.call("capture_info", &json!({})).unwrap_err();
    assert!(err.contains("No capture is open"), "{err}");
    assert!(bridge.call("seek", &json!({ "frame": 3 })).is_err());
    assert!(bridge.call("no_such_tool", &json!({})).is_err());
}

#[test]
fn questions_about_the_open_capture_are_answered() {
    let (_, _, bridge) = open();
    let info = bridge.call("capture_info", &json!({})).unwrap();
    assert_eq!(info["entities"], 37);
    let logs = bridge.call("search_logs", &json!({ "text": "state changed", "limit": 2 })).unwrap();
    assert_eq!(logs["entries"].as_array().unwrap().len(), 2);
    let err = bridge.call("get_entity", &json!({ "entity": "nobody" })).unwrap_err();
    assert!(err.contains("no entity matches"), "{err}");
}

#[test]
fn an_agent_can_drive_the_view() {
    let (ctx, mut app, bridge) = open();
    assert_eq!(state(&bridge)["capture"], "sample.vrd");

    let sought = bridge.call("seek", &json!({ "frame": 300 })).unwrap();
    assert_eq!(sought["frame"], 300);
    let by_time = bridge.call("seek", &json!({ "time": 10.0 })).unwrap();
    assert!(by_time["frame"].as_i64().unwrap() > 0);
    assert!(bridge.call("seek", &json!({})).is_err());
    bridge.call("seek", &json!({ "frame": 300 })).unwrap();

    let picked = bridge.call("select_entity", &json!({ "entity": "PlayerCharacter" })).unwrap();
    assert_eq!(picked["selected"]["name"], "PlayerCharacter");
    bridge.call("show_panel", &json!({ "panel": "entity_timelines" })).unwrap();
    assert!(bridge.call("show_panel", &json!({ "panel": "nope" })).is_err());
    bridge.call("zoom_time", &json!({ "start": 5.0, "end": 15.0 })).unwrap();
    bridge.call("toggle_pin", &json!({ "entity": "PlayerCharacter" })).unwrap();

    // Nothing happens until the app runs a frame.
    assert_eq!(app.playback().frame(), 0);
    frames(&ctx, &mut app, 3);

    assert_eq!(app.playback().frame(), 300);
    assert_eq!(app.selected(), app.replay().unwrap().entities().find(|e| e.name == "PlayerCharacter").map(|e| e.id));
    assert_eq!(app.bottom_tab(), BottomTab::EntityTimelines);
    assert!((app.time_window().start() - 5.0).abs() < 1e-6 && (app.time_window().end() - 15.0).abs() < 1e-6);
    assert!(app.graphs().pins().as_slice().len() + app.timelines().pins().as_slice().len() > 0);

    let now = state(&bridge);
    assert_eq!(now["frame"], 300);
    assert_eq!(now["panel"], "entity_timelines");
    assert_eq!(now["selected_entity"]["name"], "PlayerCharacter");

    bridge.call("zoom_time", &json!({})).unwrap();
    bridge.call("set_playback", &json!({ "playing": true, "speed": 2.0 })).unwrap();
    frames(&ctx, &mut app, 2);
    assert!(app.time_window().is_full() || app.time_window().range() > 10.0);
    assert!(app.playback().is_playing());
    assert!((app.playback().speed() - 2.0).abs() < 1e-9);
    assert!(bridge.call("set_playback", &json!({})).is_err());
}
