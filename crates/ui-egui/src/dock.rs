//! The dockable panels: which panes exist, where they start out, and how each is drawn.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Id, RichText, Ui, WidgetText};
use egui_dock::tab_viewer::OnCloseResponse;
use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabViewer};
use vrd_engine::{Playback, TimeWindow};
use vrd_model::Replay;

use crate::app::BottomTab;
use crate::entities::EntityTree;
use crate::graphs::GraphView;
use crate::logs::LogView;
use crate::pins::Pins;
use crate::properties;
use crate::timelines::TimelineView;
use crate::viewport::Viewport;

/// One panel the user can arrange. Graphs and timelines can have several, told apart by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Pane {
    Viewport,
    Entities,
    Properties,
    Logs,
    Graphs(u32),
    Timelines(u32),
}

impl Pane {
    /// The bottom-panel kind this pane belongs to, if it is one of the analysis panels.
    pub fn kind(self) -> Option<BottomTab> {
        match self {
            Pane::Logs => Some(BottomTab::Logs),
            Pane::Graphs(_) => Some(BottomTab::Graphs),
            Pane::Timelines(_) => Some(BottomTab::Timelines),
            _ => None,
        }
    }

    pub fn title(self) -> String {
        match self {
            Pane::Viewport => "3D".into(),
            Pane::Entities => "Entities".into(),
            Pane::Properties => "Properties".into(),
            Pane::Logs => "Logs".into(),
            Pane::Graphs(0) => "Graphs".into(),
            Pane::Graphs(n) => format!("Graphs {}", n + 1),
            Pane::Timelines(0) => "Timelines".into(),
            Pane::Timelines(n) => format!("Timelines {}", n + 1),
        }
    }
}

/// Entities, properties and logs on the sides and below the viewport, with the first graphs and
/// timelines views next to the logs.
pub fn default_layout() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Viewport]);
    let tree = dock.main_surface_mut();
    let [top, _] = tree.split_below(NodeIndex::root(), 0.68, vec![Pane::Logs, Pane::Graphs(0), Pane::Timelines(0)]);
    let [middle, _] = tree.split_left(top, 0.2, vec![Pane::Entities]);
    tree.split_right(middle, 0.76, vec![Pane::Properties]);
    dock
}

/// Makes `pane` the visible tab of its group, adding it next to its siblings if it was closed.
pub fn reveal(dock: &mut DockState<Pane>, pane: Pane) {
    if let Some((surface, node, tab)) = dock.find_tab(&pane) {
        dock.set_active_tab((surface, node, tab));
        dock.set_focused_node_and_surface((surface, node));
        return;
    }
    // Next to the other analysis panels when there are any, otherwise wherever the user last was.
    let sibling = pane
        .kind()
        .and_then(|_| dock.iter_all_tabs().find(|(_, other)| other.kind().is_some()).map(|(location, _)| location));
    if let Some((surface, node)) = sibling {
        dock.set_focused_node_and_surface((surface, node));
    } else if dock.focused_leaf().is_none() {
        dock.set_focused_node_and_surface((SurfaceIndex::main(), NodeIndex::root()));
    }
    dock.push_to_focused_leaf(pane);
}

/// The pane with keyboard focus, if any.
pub fn focused(dock: &mut DockState<Pane>) -> Option<Pane> {
    dock.find_active_focused().map(|(_, pane)| *pane)
}

/// All the graph and timeline views, each with its own settings.
pub struct Views {
    pub graphs: BTreeMap<u32, GraphView>,
    pub timelines: BTreeMap<u32, TimelineView>,
    next: u32,
}

impl Default for Views {
    fn default() -> Self {
        Self {
            graphs: BTreeMap::from([(0, GraphView::default())]),
            timelines: BTreeMap::from([(0, TimelineView::default())]),
            next: 1,
        }
    }
}

impl Views {
    pub fn add_graphs(&mut self, replay: Option<&Replay>) -> u32 {
        let id = self.next;
        self.next += 1;
        let mut view = GraphView::default();
        if let Some(replay) = replay {
            view.set_replay(replay);
        }
        self.graphs.insert(id, view);
        id
    }

    pub fn add_timelines(&mut self, replay: Option<&Replay>) -> u32 {
        let id = self.next;
        self.next += 1;
        let mut view = TimelineView::default();
        if let Some(replay) = replay {
            view.set_replay(replay);
        }
        self.timelines.insert(id, view);
        id
    }

    /// Forgets the extra views (the first of each kind stays) and resets the rest for a new capture.
    pub fn set_replay(&mut self, replay: &Replay) {
        self.graphs.retain(|&id, _| id == 0);
        self.timelines.retain(|&id, _| id == 0);
        self.next = 1;
        for view in self.graphs.values_mut() {
            view.set_replay(replay);
        }
        for view in self.timelines.values_mut() {
            view.set_replay(replay);
        }
    }
}

/// What the panels ask the app to do once they are drawn.
#[derive(Default)]
pub struct Requests {
    /// Show this panel (an entity was pinned and wants a place to be seen).
    pub reveal: Option<Pane>,
}

/// Draws each pane with the app's state.
pub struct Viewer<'a> {
    pub replay: &'a Replay,
    pub playback: &'a mut Playback,
    pub selected: &'a mut Option<i32>,
    pub window: &'a mut TimeWindow,
    pub pins: &'a mut Pins,
    pub entities: &'a mut EntityTree,
    pub logs: &'a mut LogView,
    pub viewport: &'a mut Viewport,
    pub views: &'a mut Views,
    pub shown: &'a BTreeSet<Pane>,
    pub requests: Requests,
}

impl TabViewer for Viewer<'_> {
    type Tab = Pane;

    fn title(&mut self, pane: &mut Pane) -> WidgetText {
        RichText::new(pane.title()).into()
    }

    fn id(&mut self, pane: &mut Pane) -> Id {
        Id::new(("pane", *pane))
    }

    fn is_closeable(&self, pane: &Pane) -> bool {
        *pane != Pane::Viewport
    }

    fn on_close(&mut self, pane: &mut Pane) -> OnCloseResponse {
        match *pane {
            Pane::Graphs(id) if id != 0 => {
                self.views.graphs.remove(&id);
            }
            Pane::Timelines(id) if id != 0 => {
                self.views.timelines.remove(&id);
            }
            _ => {}
        }
        OnCloseResponse::Close
    }

    fn scroll_bars(&self, _: &Pane) -> [bool; 2] {
        [false, false]
    }

    fn clear_background(&self, pane: &Pane) -> bool {
        *pane != Pane::Viewport
    }

    fn ui(&mut self, ui: &mut Ui, pane: &mut Pane) {
        let frame = self.playback.frame();
        let replay = self.replay;
        if *pane == Pane::Viewport {
            if let Some(id) = self.viewport.ui(ui, replay, frame, *self.selected).select {
                *self.selected = Some(id);
            }
            return;
        }

        egui::Frame::new().inner_margin(4.0).show(ui, |ui| match *pane {
            Pane::Viewport => {}
            Pane::Entities => {
                let action = self.entities.ui(
                    ui,
                    replay,
                    frame,
                    self.selected,
                    &mut self.viewport.options.hidden_entities,
                    self.pins.as_slice(),
                );
                if let Some(id) = action.focus {
                    *self.selected = Some(id);
                    self.viewport.focus(replay, id, frame);
                }
                if let Some(id) = action.toggle_pin {
                    self.pins.toggle(id);
                    // Show the entity where it has something to show.
                    let only_text = replay.entity(id).is_some_and(|e| e.values.is_empty() && !e.params.is_empty());
                    let wanted = if only_text { self.views.first_timelines() } else { self.views.first_graphs() };
                    if !self.shown.iter().any(|p| p.kind() == wanted.kind()) {
                        self.requests.reveal = Some(wanted);
                    }
                }
            }
            Pane::Properties => properties::ui(ui, replay, *self.selected, frame),
            Pane::Logs => {
                let action = self.logs.ui(ui, replay, frame, *self.selected);
                if let Some(entity) = action.select {
                    *self.selected = Some(entity);
                }
                if let Some(target) = action.seek_to {
                    self.playback.pause();
                    self.playback.seek_frame(replay, target);
                }
            }
            Pane::Graphs(id) => {
                if let Some(view) = self.views.graphs.get_mut(&id) {
                    view.ui(ui, replay, self.window, self.playback, self.selected, self.pins);
                }
            }
            Pane::Timelines(id) => {
                if let Some(view) = self.views.timelines.get_mut(&id) {
                    view.ui(ui, replay, self.window, self.playback, self.selected, self.pins);
                }
            }
        });
    }
}

impl Views {
    fn first_graphs(&self) -> Pane {
        Pane::Graphs(self.graphs.keys().next().copied().unwrap_or(0))
    }

    fn first_timelines(&self) -> Pane {
        Pane::Timelines(self.timelines.keys().next().copied().unwrap_or(0))
    }
}
