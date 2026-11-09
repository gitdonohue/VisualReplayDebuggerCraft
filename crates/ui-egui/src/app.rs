use egui::{Color32, Key, KeyboardShortcut, Modifiers, RichText};
use egui_dock::{DockArea, DockState};
use vrd_engine::{Playback, TimeWindow};
use vrd_model::Replay;

use crate::dock::{self, Pane, Views};
use crate::entities::EntityTree;
use crate::entity_timelines::EntityTimelineView;
use crate::graphs::GraphView;
use crate::loader::{self, Inbox, Incoming};
use crate::logs::LogView;
use crate::timelines::TimelineView;
use crate::transport;
use crate::viewport::Viewport;

const APP_NAME: &str = "Visual Replay Debugger";
const OPEN_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

/// What the panel under the viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomTab {
    #[default]
    Logs,
    Graphs,
    Timelines,
    /// One row per entity: when it lived and when it logged.
    EntityTimelines,
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
    /// The graphs and timelines views, each with its own settings.
    views: Views,
    entity_timelines: EntityTimelineView,
    /// Where the panels are, and which are open.
    dock: DockState<Pane>,
    /// Panels moved out of the dock into windows of their own.
    detached: Vec<Pane>,
    /// The graphs and timelines views that last had focus.
    active_graphs: u32,
    active_timelines: u32,
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
            views: Views::default(),
            entity_timelines: EntityTimelineView::default(),
            dock: dock::default_layout(),
            detached: Vec::new(),
            active_graphs: 0,
            active_timelines: 0,
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
                self.views.set_replay(&replay);
                self.entity_timelines.set_replay(&replay);
                self.dock = dock::default_layout();
                self.detached.clear();
                self.active_graphs = 0;
                self.active_timelines = 0;
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

    /// Shows a panel of this kind, opening it if it was closed.
    pub fn set_bottom_tab(&mut self, tab: BottomTab) {
        self.bottom_tab = tab;
        let pane = match tab {
            BottomTab::Logs => Pane::Logs,
            BottomTab::Graphs => Pane::Graphs(self.active_graphs),
            BottomTab::Timelines => Pane::Timelines(self.active_timelines),
            BottomTab::EntityTimelines => Pane::EntityTimelines,
        };
        if !self.detached.contains(&pane) {
            dock::reveal(&mut self.dock, pane);
        }
    }

    /// Moves a panel out of the dock into a window of its own. Closing that window docks it again.
    pub fn detach(&mut self, tab: BottomTab) {
        let pane = match tab {
            BottomTab::Logs => Pane::Logs,
            BottomTab::Graphs => Pane::Graphs(self.active_graphs),
            BottomTab::Timelines => Pane::Timelines(self.active_timelines),
            BottomTab::EntityTimelines => Pane::EntityTimelines,
        };
        if let Some(location) = self.dock.find_tab(&pane) {
            self.dock.remove_tab(location);
        }
        if !self.detached.contains(&pane) {
            self.detached.push(pane);
        }
    }

    /// How many panels are in windows of their own.
    pub fn detached_count(&self) -> usize {
        self.detached.len()
    }

    /// Opens another graphs view, with its own settings, and returns its number.
    pub fn add_graphs_view(&mut self) -> u32 {
        let id = self.views.add_graphs(self.session.as_ref().map(|s| &s.replay));
        self.active_graphs = id;
        dock::reveal(&mut self.dock, Pane::Graphs(id));
        id
    }

    /// Opens another timelines view, with its own settings, and returns its number.
    pub fn add_timelines_view(&mut self) -> u32 {
        let id = self.views.add_timelines(self.session.as_ref().map(|s| &s.replay));
        self.active_timelines = id;
        dock::reveal(&mut self.dock, Pane::Timelines(id));
        id
    }

    pub fn graph_view_count(&self) -> usize {
        self.views.graphs.len()
    }

    pub fn timeline_view_count(&self) -> usize {
        self.views.timelines.len()
    }

    /// Puts the panels back where they start out.
    pub fn reset_layout(&mut self) {
        self.dock = dock::default_layout();
        self.detached.clear();
        self.views.graphs.retain(|&id, _| id == 0);
        self.views.timelines.retain(|&id, _| id == 0);
        self.active_graphs = 0;
        self.active_timelines = 0;
    }

    /// The span of time the graphs show.
    pub fn time_window(&self) -> &TimeWindow {
        &self.window
    }

    pub fn time_window_mut(&mut self) -> &mut TimeWindow {
        &mut self.window
    }

    /// The graphs view that last had focus.
    pub fn graphs(&self) -> &GraphView {
        self.views.graphs.get(&self.active_graphs).unwrap_or(&self.views.graphs[&0])
    }

    pub fn graphs_mut(&mut self) -> &mut GraphView {
        let id = if self.views.graphs.contains_key(&self.active_graphs) { self.active_graphs } else { 0 };
        self.views.graphs.get_mut(&id).expect("the first graphs view always exists")
    }

    /// The timelines view that last had focus.
    pub fn entity_timelines(&self) -> &EntityTimelineView {
        &self.entity_timelines
    }

    pub fn entity_timelines_mut(&mut self) -> &mut EntityTimelineView {
        &mut self.entity_timelines
    }

    /// The timelines view that last had focus.
    pub fn timelines(&self) -> &TimelineView {
        self.views.timelines.get(&self.active_timelines).unwrap_or(&self.views.timelines[&0])
    }

    pub fn timelines_mut(&mut self) -> &mut TimelineView {
        let id = if self.views.timelines.contains_key(&self.active_timelines) { self.active_timelines } else { 0 };
        self.views.timelines.get_mut(&id).expect("the first timelines view always exists")
    }

    /// Pins an entity in the graphs and timelines views that last had focus, or unpins it. Every view
    /// keeps its own pins.
    pub fn toggle_pin(&mut self, entity: i32) {
        self.graphs_mut().pins_mut().toggle(entity);
        self.timelines_mut().pins_mut().toggle(entity);
    }

    /// Moves the playhead to the next or previous change of what the timelines show.
    pub fn step_timeline_event(&mut self, forward: bool) {
        let Some(session) = &self.session else {
            return;
        };
        let t = self.playback.time();
        if let Some(event) = self.timelines().neighbouring_event(self.selected, self.timelines().pins(), t, forward) {
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
            views,
            entity_timelines,
            bottom_tab,
            window,
            entities,
            viewport,
            dock,
            detached,
            active_graphs,
            active_timelines,
            ..
        } = self;

        match session {
            Some(Session { replay, .. }) => {
                egui::TopBottomPanel::bottom("transport").show(ctx, |ui| {
                    ui.add_space(4.0);
                    transport::ui(ui, replay, playback);
                    ui.add_space(4.0);
                });

                let shown =
                    dock.iter_leaves().filter_map(|(_, leaf)| leaf.tabs().get(leaf.active.0).copied()).collect();
                let mut viewer = dock::Viewer {
                    replay,
                    playback,
                    selected,
                    window,
                    entities,
                    logs,
                    entity_timelines,
                    viewport,
                    views,
                    active_graphs: *active_graphs,
                    active_timelines: *active_timelines,
                    shown: &shown,
                    requests: dock::Requests::default(),
                };
                let mut style = egui_dock::Style::from_egui(&ctx.style());
                style.tab.tab_body.inner_margin = egui::Margin::ZERO;
                DockArea::new(dock).style(style).show_leaf_close_all_buttons(false).show(ctx, &mut viewer);
                let closed = dock::show_detached(ctx, &mut viewer, detached);
                let requests = std::mem::take(&mut viewer.requests);
                for pane in closed {
                    detached.retain(|p| *p != pane);
                    dock::reveal(dock, pane);
                }
                if let Some(pane) = requests.detach {
                    if let Some(location) = dock.find_tab(&pane) {
                        dock.remove_tab(location);
                    }
                    if !detached.contains(&pane) {
                        detached.push(pane);
                    }
                }

                if let Some(pane) = requests.reveal {
                    dock::reveal(dock, pane);
                }
                match dock::focused(dock) {
                    Some(Pane::Graphs(id)) => *active_graphs = id,
                    Some(Pane::Timelines(id)) => *active_timelines = id,
                    _ => {}
                }
                if let Some(kind) = dock::focused(dock).and_then(Pane::kind) {
                    *bottom_tab = kind;
                }
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
                if self.session.is_some() {
                    ui.menu_button("View", |ui| {
                        for pane in [Pane::Entities, Pane::Properties, Pane::Logs] {
                            let open = self.dock.find_tab(&pane).is_some() || self.detached.contains(&pane);
                            if ui.selectable_label(open, pane.title()).clicked() {
                                if self.detached.contains(&pane) {
                                    self.detached.retain(|p| *p != pane);
                                } else if open {
                                    if let Some((surface, node, tab)) = self.dock.find_tab(&pane) {
                                        self.dock.remove_tab((surface, node, tab));
                                    }
                                } else {
                                    dock::reveal(&mut self.dock, pane);
                                }
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("New graphs view").clicked() {
                            self.add_graphs_view();
                            ui.close();
                        }
                        if ui.button("New timelines view").clicked() {
                            self.add_timelines_view();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Reset layout").clicked() {
                            self.reset_layout();
                            ui.close();
                        }
                    });
                }
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
        let mut tab = None;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::G) {
                tab = Some(BottomTab::Graphs);
            }
            if i.consume_key(Modifiers::NONE, Key::T) {
                tab = Some(BottomTab::Timelines);
            }
            if i.consume_key(Modifiers::NONE, Key::E) {
                tab = Some(BottomTab::EntityTimelines);
            }
            if i.consume_key(Modifiers::NONE, Key::L) {
                tab = Some(BottomTab::Logs);
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
        if let Some(tab) = tab {
            self.set_bottom_tab(tab);
        }
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
