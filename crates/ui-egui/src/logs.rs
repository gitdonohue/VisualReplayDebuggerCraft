use egui::{Align, Layout, RichText, Sense, TextStyle};
use egui_extras::{Column, TableBuilder};
use vrd_model::Replay;

use crate::log_filter::LogFilter;
use crate::theme::text_color;

/// What the user asked for by interacting with the log table.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LogAction {
    pub seek_to: Option<i32>,
    pub select: Option<i32>,
}

/// State of the logs panel.
pub struct LogView {
    pub filter: LogFilter,
    pub only_selected: bool,
    pub follow: bool,
    filtered: Vec<u32>,
    dirty: bool,
    last_followed_frame: Option<i32>,
}

impl Default for LogView {
    fn default() -> Self {
        Self {
            filter: LogFilter::default(),
            only_selected: false,
            follow: true,
            filtered: Vec::new(),
            dirty: true,
            last_followed_frame: None,
        }
    }
}

impl LogView {
    /// Forgets everything tied to the previously loaded replay.
    pub fn reset(&mut self) {
        self.filter = LogFilter::default();
        self.only_selected = false;
        self.filtered.clear();
        self.dirty = true;
        self.last_followed_frame = None;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, replay: &Replay, current_frame: i32, selected: Option<i32>) -> LogAction {
        let mut action = LogAction::default();

        self.toolbar(ui, replay, selected);

        let wanted_entity = if self.only_selected { selected } else { None };
        if self.filter.only_entity != wanted_entity {
            self.filter.only_entity = wanted_entity;
            self.dirty = true;
        }
        if self.dirty {
            self.filtered = self.filter.apply(replay);
            self.dirty = false;
            self.last_followed_frame = None;
        }

        ui.add_space(2.0);

        if replay.logs().is_empty() {
            ui.centered_and_justified(|ui| ui.weak("This capture has no log entries."));
            return action;
        }

        let logs = replay.logs();
        let dark = ui.visuals().dark_mode;
        let row_height = ui.text_style_height(&TextStyle::Body) + 4.0;

        let mut scroll_to = None;
        if self.follow && !self.filtered.is_empty() && self.last_followed_frame != Some(current_frame) {
            let after = self.filtered.partition_point(|&i| logs[i as usize].frame <= current_frame);
            scroll_to = Some(after.saturating_sub(1));
            self.last_followed_frame = Some(current_frame);
        }

        let filtered = &self.filtered;
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(56.0))
            .column(Column::exact(72.0))
            .column(Column::initial(110.0).at_least(40.0).clip(true))
            .column(Column::initial(170.0).at_least(40.0).clip(true))
            .column(Column::remainder().at_least(80.0).clip(true))
            .min_scrolled_height(0.0);
        if let Some(row) = scroll_to {
            table = table.scroll_to_row(row, Some(Align::Center));
        }

        table
            .header(20.0, |mut header| {
                for title in ["Frame", "Time", "Category", "Entity", "Message"] {
                    header.col(|ui| {
                        ui.strong(title);
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, filtered.len(), |mut row| {
                    let entry = &logs[filtered[row.index()] as usize];
                    let is_current = entry.frame == current_frame;
                    let is_future = entry.frame > current_frame;
                    row.set_selected(is_current);

                    let mut color = text_color(entry.color, dark);
                    if is_future {
                        color = color.gamma_multiply(0.5);
                    }
                    let cell = |ui: &mut egui::Ui, text: &str, mono: bool| {
                        let mut t = RichText::new(text).color(color);
                        if mono {
                            t = t.monospace();
                        }
                        ui.add(egui::Label::new(t).truncate()).on_hover_text(text);
                    };

                    row.col(|ui| cell(ui, &entry.frame.to_string(), true));
                    row.col(|ui| cell(ui, &format!("{:.3}s", replay.time_for_frame(entry.frame)), true));
                    row.col(|ui| cell(ui, &entry.category, false));
                    row.col(|ui| {
                        let name = replay
                            .entity(entry.entity)
                            .map_or_else(|| format!("#{}", entry.entity), |e| e.display_name());
                        cell(ui, &name, false);
                    });
                    row.col(|ui| cell(ui, &entry.message, false));

                    if row.response().clicked() {
                        action.seek_to = Some(entry.frame);
                        action.select = Some(entry.entity);
                    }
                });
            });

        action
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, replay: &Replay, selected: Option<i32>) {
        ui.horizontal_wrapped(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.filter.text).hint_text("Filter logs…").desired_width(200.0);
            if ui.add(edit).changed() {
                self.dirty = true;
            }
            if !self.filter.text.is_empty() && ui.small_button("✕").on_hover_text("Clear filter").clicked() {
                self.filter.text.clear();
                self.dirty = true;
            }

            ui.menu_button("Categories", |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        self.filter.hidden_categories.clear();
                        self.dirty = true;
                    }
                    if ui.small_button("None").clicked() {
                        self.filter.hidden_categories = replay.log_categories().clone();
                        self.dirty = true;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for category in replay.log_categories() {
                        let mut shown = !self.filter.hidden_categories.contains(category);
                        let label = if category.is_empty() { "(none)" } else { category.as_str() };
                        if ui.checkbox(&mut shown, label).changed() {
                            if shown {
                                self.filter.hidden_categories.remove(category);
                            } else {
                                self.filter.hidden_categories.insert(category.clone());
                            }
                            self.dirty = true;
                        }
                    }
                });
            });

            ui.add_enabled_ui(selected.is_some() || self.only_selected, |ui| {
                ui.checkbox(&mut self.only_selected, "Selected entity only");
            });
            if ui.checkbox(&mut self.follow, "Follow playback").changed() {
                self.last_followed_frame = None;
            }

            ui.weak(format!("{} / {}", self.filtered.len(), replay.logs().len()));
        });
    }
}
