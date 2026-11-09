//! What an AI agent can do with the running app: ask questions of the open capture and drive the view.
//!
//! The browser build offers these as WebMCP tools. [`Bridge`] is the app's side of that: it holds the open
//! capture, takes commands to run on the next frame, and reports what the app shows. It knows nothing
//! about the browser, so it is tested natively.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{Value, json};
use vrd_model::Replay;

use crate::app::BottomTab;

/// Something an agent asked the app to do, run at the start of the next frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    SeekFrame(i32),
    SeekTime(f64),
    Select(Option<i32>),
    Focus(i32),
    Panel(BottomTab),
    Play(bool),
    Speed(f64),
    TogglePin(i32),
    /// Show this span of time in the graphs and timelines, or everything when `None`.
    Window(Option<(f64, f64)>),
}

/// What the app showed on its last frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Status {
    pub frame: i32,
    pub time: f64,
    pub playing: bool,
    pub speed: f64,
    pub selected: Option<i32>,
    pub panel: &'static str,
    pub window: (f64, f64),
}

#[derive(Default)]
struct Inner {
    name: String,
    replay: Option<Rc<Replay>>,
    commands: Vec<Command>,
    status: Status,
    ctx: Option<egui::Context>,
}

/// A handle on the app for code outside the UI (such as the browser's WebMCP glue). Cheap to clone; not
/// for other threads.
#[derive(Clone, Default)]
pub struct Bridge(Rc<RefCell<Inner>>);

impl Bridge {
    pub(crate) fn set_replay(&self, name: &str, replay: Rc<Replay>) {
        let mut inner = self.0.borrow_mut();
        inner.name = name.to_string();
        inner.replay = Some(replay);
    }

    pub(crate) fn set_context(&self, ctx: &egui::Context) {
        let mut inner = self.0.borrow_mut();
        if inner.ctx.is_none() {
            inner.ctx = Some(ctx.clone());
        }
    }

    pub(crate) fn take_commands(&self) -> Vec<Command> {
        std::mem::take(&mut self.0.borrow_mut().commands)
    }

    pub(crate) fn publish(&self, status: Status) {
        self.0.borrow_mut().status = status;
    }

    fn send(&self, command: Command) {
        let mut inner = self.0.borrow_mut();
        inner.commands.push(command);
        if let Some(ctx) = &inner.ctx {
            ctx.request_repaint();
        }
    }

    fn replay(&self) -> Result<Rc<Replay>, String> {
        self.0
            .borrow()
            .replay
            .clone()
            .ok_or_else(|| "No capture is open. Ask the user to open a .vrd capture first.".to_string())
    }

    /// The tools on offer, with their JSON schemas: the questions about the capture, then the ones that
    /// drive the view.
    pub fn definitions() -> Vec<Value> {
        let mut tools = vrd_analysis::definitions(false);
        for tool in &mut tools {
            tool["annotations"] = json!({ "readOnlyHint": true });
        }
        let entity = json!({ "description": "Entity id (number) or name/path (text; a unique part is enough)" });
        let mut ui = vec![
            view_tool(
                "get_view_state",
                "What the debugger is showing now: the open capture, the frame and time, whether it is playing, the selected entity, the visible panel and the time span shown in the graphs.",
                &json!({}),
                &[],
                true,
            ),
            view_tool(
                "seek",
                "Move the playhead to a frame or a time in seconds (pauses playback). Give one of them.",
                &json!({
                    "frame": { "type": "integer" },
                    "time": { "type": "number", "description": "Seconds from the start" },
                }),
                &[],
                false,
            ),
            view_tool(
                "set_playback",
                "Start or stop playback and/or set its speed.",
                &json!({
                    "playing": { "type": "boolean" },
                    "speed": { "type": "number", "description": "1 is real time" },
                }),
                &[],
                false,
            ),
            view_tool(
                "select_entity",
                "Select an entity, which shows its properties and graphs, and by default point the 3D camera at it.",
                &json!({
                    "entity": entity,
                    "focus": { "type": "boolean", "description": "Move the 3D camera to it (default true)" },
                }),
                &["entity"],
                false,
            ),
            view_tool(
                "show_panel",
                "Bring a panel to the front: logs, graphs (numeric values), timelines (text parameters) or entity_timelines (when each entity lived and logged).",
                &json!({ "panel": { "type": "string", "enum": ["logs", "graphs", "timelines", "entity_timelines"] } }),
                &["panel"],
                false,
            ),
            view_tool(
                "toggle_pin",
                "Pin an entity in the graphs and timelines so it stays shown while others are selected, or unpin it.",
                &json!({ "entity": entity }),
                &["entity"],
                false,
            ),
            view_tool(
                "zoom_time",
                "Show a span of time (seconds) in the graphs and timelines. Without start and end, show the whole capture.",
                &json!({ "start": { "type": "number" }, "end": { "type": "number" } }),
                &[],
                false,
            ),
        ];
        tools.append(&mut ui);
        tools
    }

    /// Runs a tool. Questions about the capture answer from it; the others queue a change for the next frame
    /// and answer with what the view will show.
    pub fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
        if vrd_analysis::definitions(false).iter().any(|d| d["name"] == name) {
            let replay = self.replay()?;
            return vrd_analysis::call(&replay, name, args).map_err(|e| e.0);
        }
        match name {
            "get_view_state" => Ok(self.view_state()),
            "seek" => self.seek(args),
            "set_playback" => self.set_playback(args),
            "select_entity" => {
                let replay = self.replay()?;
                let id = vrd_analysis::resolve_entity(&replay, args.get("entity").unwrap_or(&Value::Null))
                    .map_err(|e| e.0)?;
                self.send(Command::Select(Some(id)));
                if args.get("focus").and_then(Value::as_bool).unwrap_or(true) {
                    self.send(Command::Focus(id));
                }
                Ok(json!({ "selected": entity_json(&replay, id) }))
            }
            "show_panel" => {
                let panel = args.get("panel").and_then(Value::as_str).unwrap_or_default();
                let tab = parse_panel(panel).ok_or_else(|| {
                    format!("unknown panel `{panel}`. Choose logs, graphs, timelines or entity_timelines")
                })?;
                self.send(Command::Panel(tab));
                Ok(json!({ "panel": panel_name(tab) }))
            }
            "toggle_pin" => {
                let replay = self.replay()?;
                let id = vrd_analysis::resolve_entity(&replay, args.get("entity").unwrap_or(&Value::Null))
                    .map_err(|e| e.0)?;
                self.send(Command::TogglePin(id));
                Ok(json!({ "toggled": entity_json(&replay, id) }))
            }
            "zoom_time" => {
                let replay = self.replay()?;
                let (start, end) = (args.get("start").and_then(Value::as_f64), args.get("end").and_then(Value::as_f64));
                match (start, end) {
                    (Some(start), Some(end)) if end > start => {
                        let total = replay.total_time();
                        let (start, end) = (start.clamp(0.0, total), end.clamp(0.0, total));
                        self.send(Command::Window(Some((start, end))));
                        Ok(json!({ "start": start, "end": end }))
                    }
                    (None, None) => {
                        self.send(Command::Window(None));
                        Ok(json!({ "start": 0.0, "end": replay.total_time() }))
                    }
                    _ => {
                        Err("give both `start` and `end` (seconds, end after start), or neither to show everything"
                            .into())
                    }
                }
            }
            other => Err(format!("unknown tool `{other}`")),
        }
    }

    fn view_state(&self) -> Value {
        let inner = self.0.borrow();
        let Some(replay) = &inner.replay else {
            return json!({ "capture": null });
        };
        let s = &inner.status;
        json!({
            "capture": inner.name,
            "frame": s.frame,
            "last_frame": replay.last_frame(),
            "time": s.time,
            "duration": replay.total_time(),
            "playing": s.playing,
            "speed": s.speed,
            "selected_entity": s.selected.map(|id| entity_json(replay, id)),
            "panel": s.panel,
            "time_window": { "start": s.window.0, "end": s.window.1 },
        })
    }

    fn seek(&self, args: &Value) -> Result<Value, String> {
        let replay = self.replay()?;
        let frame = args.get("frame").and_then(Value::as_i64);
        let time = args.get("time").and_then(Value::as_f64);
        let frame = match (frame, time) {
            (Some(f), None) => {
                self.send(Command::SeekFrame(f.clamp(0, i64::from(replay.last_frame())) as i32));
                f.clamp(0, i64::from(replay.last_frame())) as i32
            }
            (None, Some(t)) => {
                let t = t.clamp(0.0, replay.total_time());
                self.send(Command::SeekTime(t));
                replay.frame_for_time(t)
            }
            _ => return Err("give either `frame` or `time`".into()),
        };
        Ok(json!({ "frame": frame, "time": replay.time_for_frame(frame) }))
    }

    fn set_playback(&self, args: &Value) -> Result<Value, String> {
        let playing = args.get("playing").and_then(Value::as_bool);
        let speed = args.get("speed").and_then(Value::as_f64);
        if playing.is_none() && speed.is_none() {
            return Err("give `playing` and/or `speed`".into());
        }
        self.replay()?;
        if let Some(speed) = speed {
            self.send(Command::Speed(speed));
        }
        if let Some(playing) = playing {
            self.send(Command::Play(playing));
        }
        Ok(json!({ "playing": playing, "speed": speed }))
    }
}

fn view_tool(name: &str, description: &str, properties: &Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
        "annotations": { "readOnlyHint": read_only },
    })
}

fn entity_json(replay: &Replay, id: i32) -> Value {
    json!({ "id": id, "name": replay.entity(id).map(vrd_model::Entity::display_name) })
}

pub(crate) fn panel_name(tab: BottomTab) -> &'static str {
    match tab {
        BottomTab::Logs => "logs",
        BottomTab::Graphs => "graphs",
        BottomTab::Timelines => "timelines",
        BottomTab::EntityTimelines => "entity_timelines",
    }
}

fn parse_panel(name: &str) -> Option<BottomTab> {
    [BottomTab::Logs, BottomTab::Graphs, BottomTab::Timelines, BottomTab::EntityTimelines]
        .into_iter()
        .find(|tab| panel_name(*tab) == name)
}
