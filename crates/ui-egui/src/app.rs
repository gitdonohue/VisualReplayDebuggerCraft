use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText};
use vrd_engine::{Playback, TimeWindow};
use vrd_model::Replay;

use crate::entities::EntityTree;
use crate::graphs::GraphView;
use crate::loader::{self, Inbox, Incoming};
use crate::logs::LogView;
use crate::pins::Pins;
use crate::timelines::TimelineView;
use crate::viewport::Viewport;
use crate::{properties, transport};

const APP_NAME: &str = "Visual Replay Debugger";
const OPEN_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

/// What the panel under the viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomTab {
    #[default]
    Logs,
    Graphs,
    Timelines,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Warning,
    Error,
}

struct Notice {
    level: Level,
    text: String,
}

struct Session {
    name: String,
    replay: Replay,
}

/// The whole replay debugger UI. Hosts (desktop, web, tests) call [`ReplayApp::ui`] once per frame.
pub struct ReplayApp {
    session: Option<Session>,
    playback: Playback,
    selected: Option<i32>,
    logs: LogView,
    graphs: GraphView,
    timelines: TimelineView,
    /// Entities kept in the graphs and timelines whatever is selected.
    pins: Pins,
    bottom_tab: BottomTab,
    /// The span of time the graphs show.
    window: TimeWindow,
    entities: EntityTree,
    viewport: Viewport,
    inbox: Inbox,
    notice: Option<Notice>,
    title_dirty: bool,
}

impl Default for ReplayApp {
    fn default() -> Self {
        Self {
            session: None,
            playback: Playback::new(),
            selected: None,
            logs: LogView::default(),
            graphs: GraphView::default(),
            timelines: TimelineView::default(),
            pins: Pins::default(),
            bottom_tab: BottomTab::default(),
            window: TimeWindow::default(),
            entities: EntityTree::default(),
            viewport: Viewport::default(),
            inbox: Inbox::default(),
            notice: None,
            title_dirty: true,
        }
    }
}

impl ReplayApp {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads a capture, replacing the current one. Problems are reported in the UI.
    pub fn open_bytes(&mut self, name: impl Into<String>, bytes: &[u8]) {
        let name = name.into();
        match Replay::from_bytes(bytes) {
            Ok(replay) => {
                let info = replay.info();
                self.notice = if info.truncated {
                    Some(Notice {
                        level: Level::Warning,
                        text: format!(
                            "{name} ends mid-stream (still being recorded?). Showing everything that could be read{}.",
                            info.warning.as_ref().map_or(String::new(), |w| format!(" ({w})"))
                        ),
                    })
                } else {
                    None
                };

                let mut playback = Playback::new();
                playback.set_speed(self.playback.speed());
                playback.set_looping(self.playback.is_looping());
                self.playback = playback;
                self.selected = replay.root_entities().first().copied();
                self.logs.reset();
                self.graphs.set_replay(&replay);
                self.timelines.set_replay(&replay);
                self.pins.clear();
                self.window = TimeWindow::new(replay.total_time());
                self.entities = EntityTree::default();
                self.viewport.set_replay(&replay);
                self.session = Some(Session { name, replay });
                self.title_dirty = true;
            }
            Err(e) => self.fail(format!("Cannot open {name}: {e}")),
        }
    }

    /// Loads a capture from disk.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: &std::path::Path) {
        self.receive(loader::read_path(path));
    }

    /// Enables 3D rendering with the host's wgpu device. Without this the viewport shows a notice.
    pub fn attach_wgpu(&mut self, state: egui_wgpu::RenderState) {
        self.viewport.attach_gpu(state);
    }

    /// A handle other threads or async tasks can use to hand this app a capture to open.
    pub fn inbox(&self) -> Inbox {
        self.inbox.clone()
    }

    /// Whether the 3D viewport has a GPU to render with.
    pub fn has_3d(&self) -> bool {
        self.viewport.has_gpu()
    }

    /// Where the 3D viewport was last drawn, in points.
    pub fn viewport_rect(&self) -> egui::Rect {
        self.viewport.rect()
    }

    /// The 3D viewport's display options.
    pub fn view_options(&self) -> &vrd_scene::ViewOptions {
        &self.viewport.options
    }

    /// Selects an entity and points the 3D camera at it.
    pub fn focus_entity(&mut self, id: i32) {
        self.selected = Some(id);
        if let Some(session) = &self.session {
            self.viewport.focus(&session.replay, id, self.playback.frame());
        }
    }

    /// What the panel under the viewport shows.
    pub fn bottom_tab(&self) -> BottomTab {
        self.bottom_tab
    }

    pub fn set_bottom_tab(&mut self, tab: BottomTab) {
        self.bottom_tab = tab;
    }

    /// The span of time the graphs show.
    pub fn time_window(&self) -> &TimeWindow {
        &self.window
    }

    pub fn time_window_mut(&mut self) -> &mut TimeWindow {
        &mut self.window
    }

    pub fn graphs(&self) -> &GraphView {
        &self.graphs
    }

    pub fn graphs_mut(&mut self) -> &mut GraphView {
        &mut self.graphs
    }

    pub fn timelines(&self) -> &TimelineView {
        &self.timelines
    }

    pub fn timelines_mut(&mut self) -> &mut TimelineView {
        &mut self.timelines
    }

    /// Pins an entity in the graphs and timelines, or unpins it.
    pub fn toggle_pin(&mut self, entity: i32) {
        self.pins.toggle(entity);
    }

    pub fn pins(&self) -> &Pins {
        &self.pins
    }

    /// Moves the playhead to the next or previous change of what the timelines show.
    pub fn step_timeline_event(&mut self, forward: bool) {
        let Some(session) = &self.session else {
            return;
        };
        let t = self.playback.time();
        if let Some(event) = self.timelines.neighbouring_event(self.selected, &self.pins, t, forward) {
            self.playback.seek_time(&session.replay, event);
        }
    }

    /// The 3D viewport's camera.
    pub fn viewport_camera(&self) -> &vrd_scene::Camera {
        self.viewport.camera()
    }

    pub fn replay(&self) -> Option<&Replay> {
        self.session.as_ref().map(|s| &s.replay)
    }

    pub fn playback(&self) -> &Playback {
        &self.playback
    }

    pub fn playback_mut(&mut self) -> &mut Playback {
        &mut self.playback
    }

    pub fn selected(&self) -> Option<i32> {
        self.selected
    }

    pub fn select(&mut self, id: Option<i32>) {
        self.selected = id;
    }

    /// Pauses and jumps to the start of `frame`.
    pub fn seek_frame(&mut self, frame: i32) {
        if let Some(session) = &self.session {
            self.playback.pause();
            self.playback.seek_frame(&session.replay, frame);
        }
    }

    fn fail(&mut self, text: String) {
        self.notice = Some(Notice { level: Level::Error, text });
    }

    fn receive(&mut self, incoming: Incoming) {
        match incoming {
            Incoming::File { name, bytes } => self.open_bytes(name, &bytes),
            Incoming::Error(text) => self.fail(text),
        }
    }

    /// Draws the UI and advances playback. Call once per frame.
    pub fn ui(&mut self, ctx: &egui::Context) {
        for incoming in self.inbox.drain() {
            self.receive(incoming);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            self.receive(loader::read_dropped(file));
        }

        self.handle_keys(ctx);

        if let Some(session) = &self.session {
            let dt = f64::from(ctx.input(|i| i.stable_dt));
            if self.playback.advance(&session.replay, dt) || self.playback.is_playing() {
                ctx.request_repaint();
            }
            self.window.observe_playhead(self.playback.time());
        }

        if self.title_dirty {
            self.title_dirty = false;
            let title = match &self.session {
                Some(s) => format!("{} — {APP_NAME}", s.name),
                None => APP_NAME.to_string(),
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }

        self.top_bar(ctx);

        let Self {
            session,
            playback,
            selected,
            logs,
            graphs,
            timelines,
            pins,
            bottom_tab,
            window,
            entities,
            viewport,
            ..
        } = self;

        match session {
            Some(Session { replay, .. }) => {
                egui::TopBottomPanel::bottom("transport").show(ctx, |ui| {
                    ui.add_space(4.0);
                    transport::ui(ui, replay, playback);
                    ui.add_space(4.0);
                });

                let frame = playback.frame();
                egui::SidePanel::left("entities").default_width(260.0).width_range(160.0..=600.0).show(ctx, |ui| {
                    ui.heading("Entities");
                    let action = entities.ui(
                        ui,
                        replay,
                        frame,
                        selected,
                        &mut viewport.options.hidden_entities,
                        pins.as_slice(),
                    );
                    if let Some(id) = action.focus {
                        *selected = Some(id);
                        viewport.focus(replay, id, frame);
                    }
                    if let Some(id) = action.toggle_pin {
                        pins.toggle(id);
                        // Show the entity where it has something to show.
                        if *bottom_tab == BottomTab::Logs {
                            let only_text =
                                replay.entity(id).is_some_and(|e| e.values.is_empty() && !e.params.is_empty());
                            *bottom_tab = if only_text { BottomTab::Timelines } else { BottomTab::Graphs };
                        }
                    }
                });
                egui::SidePanel::right("properties").default_width(320.0).width_range(200.0..=700.0).show(ctx, |ui| {
                    ui.heading("Properties");
                    properties::ui(ui, replay, *selected, frame);
                });
                egui::TopBottomPanel::bottom("bottom")
                    .resizable(true)
                    .default_height(260.0)
                    .height_range(80.0..=2000.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(bottom_tab, BottomTab::Logs, RichText::new("Logs").heading())
                                .on_hover_text("Log entries (L)");
                            ui.selectable_value(bottom_tab, BottomTab::Graphs, RichText::new("Graphs").heading())
                                .on_hover_text("Numeric values over time (G)");
                            ui.selectable_value(bottom_tab, BottomTab::Timelines, RichText::new("Timelines").heading())
                                .on_hover_text("Text parameters over time (T)");
                        });
                        match bottom_tab {
                            BottomTab::Logs => {
                                let action = logs.ui(ui, replay, frame, *selected);
                                if let Some(entity) = action.select {
                                    *selected = Some(entity);
                                }
                                if let Some(target) = action.seek_to {
                                    playback.pause();
                                    playback.seek_frame(replay, target);
                                }
                            }
                            BottomTab::Graphs => graphs.ui(ui, replay, window, playback, selected, pins),
                            BottomTab::Timelines => timelines.ui(ui, replay, window, playback, selected, pins),
                        }
                    });
                egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0)).show(
                    ctx,
                    |ui| {
                        if let Some(id) = viewport.ui(ui, replay, frame, *selected).select {
                            *selected = Some(id);
                        }
                    },
                );
            }
            None => {
                let mut open = false;
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() / 3.0).max(20.0));
                        ui.heading(APP_NAME);
                        ui.add_space(8.0);
                        ui.label("Drop a .vrd capture here, or");
                        ui.add_space(8.0);
                        if ui.button("Open a capture…").clicked() {
                            open = true;
                        }
                    });
                });
                if open {
                    loader::open_dialog(&self.inbox, ctx);
                }
            }
        }

        drop_overlay(ctx);
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    let open = egui::Button::new("Open…").shortcut_text(ctx.format_shortcut(&OPEN_SHORTCUT));
                    if ui.add(open).clicked() {
                        loader::open_dialog(&self.inbox, ctx);
                        ui.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.separator();
                if let Some(session) = &self.session {
                    let r = &session.replay;
                    ui.label(RichText::new(&session.name).strong());
                    ui.weak(format!(
                        "{} frames · {:.1} s · {} entities · {} logs",
                        r.frame_count(),
                        r.total_time(),
                        r.entity_count(),
                        r.logs().len()
                    ));
                }

                let mut dismiss = false;
                if let Some(notice) = &self.notice {
                    ui.separator();
                    let color = match notice.level {
                        Level::Warning => Color32::from_rgb(230, 170, 40),
                        Level::Error => Color32::from_rgb(235, 90, 80),
                    };
                    ui.label(RichText::new(&notice.text).color(color));
                    dismiss = ui.small_button("✕").clicked();
                }
                if dismiss {
                    self.notice = None;
                }
            });
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&OPEN_SHORTCUT)) {
            loader::open_dialog(&self.inbox, ctx);
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let Some(session) = &self.session else {
            return;
        };
        let replay = &session.replay;
        let playback = &mut self.playback;
        let tab = &mut self.bottom_tab;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::G) {
                *tab = BottomTab::Graphs;
            }
            if i.consume_key(Modifiers::NONE, Key::T) {
                *tab = BottomTab::Timelines;
            }
            if i.consume_key(Modifiers::NONE, Key::L) {
                *tab = BottomTab::Logs;
            }
            if i.consume_key(Modifiers::NONE, Key::Space) {
                playback.toggle(replay);
            }
            let big = if i.modifiers.shift { 10 } else { 1 };
            if i.consume_key(Modifiers::NONE, Key::ArrowRight) || i.consume_key(Modifiers::SHIFT, Key::ArrowRight) {
                playback.step(replay, big);
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowLeft) || i.consume_key(Modifiers::SHIFT, Key::ArrowLeft) {
                playback.step(replay, -big);
            }
            if i.consume_key(Modifiers::NONE, Key::Home) {
                playback.rewind(replay);
            }
            if i.consume_key(Modifiers::NONE, Key::End) {
                playback.pause();
                playback.seek_frame(replay, replay.last_frame());
            }
            if i.consume_key(Modifiers::NONE, Key::PageDown) {
                playback.step_to_next_log(replay);
            }
            if i.consume_key(Modifiers::NONE, Key::PageUp) {
                playback.step_to_previous_log(replay);
            }
        });
    }
}

fn drop_overlay(ctx: &egui::Context) {
    if !ctx.input(|i| !i.raw.hovered_files.is_empty()) {
        return;
    }
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay")));
    painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
    painter.text(
        screen.center(),
        egui::Align2::CENTER_CENTER,
        "Drop a .vrd capture to open it",
        egui::FontId::proportional(28.0),
        Color32::WHITE,
    );
}
