use std::collections::BTreeSet;

use egui::collapsing_header::CollapsingState;
use egui::{RichText, ScrollArea};
use vrd_model::Replay;

use crate::log_filter::contains_ignore_case;

/// What the user asked for in the entity tree besides selecting.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EntityAction {
    /// Select this entity and look at it in the viewport.
    pub focus: Option<i32>,
}

/// The entity hierarchy with a search box.
#[derive(Default)]
pub struct EntityTree {
    pub filter: String,
}

impl EntityTree {
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        replay: &Replay,
        frame: i32,
        selected: &mut Option<i32>,
        hidden: &mut BTreeSet<i32>,
    ) -> EntityAction {
        let mut action = EntityAction::default();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search entities…")
                    .desired_width(ui.available_width() - 24.0),
            );
            if !self.filter.is_empty() && ui.small_button("✕").clicked() {
                self.filter.clear();
            }
        });
        ui.add_space(2.0);

        if replay.entity_count() == 0 {
            ui.weak("No entities in this capture.");
            return action;
        }

        let mut ctx = Ctx { replay, frame, selected, hidden, action: &mut action };
        let needle = self.filter.trim().to_ascii_lowercase();
        if needle.is_empty() {
            ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
                for &id in replay.root_entities() {
                    node(ui, &mut ctx, id, 0);
                }
            });
        } else {
            let matches: Vec<i32> = replay
                .entities()
                .filter(|e| {
                    contains_ignore_case(&e.name, &needle)
                        || contains_ignore_case(&e.path, &needle)
                        || contains_ignore_case(&e.type_name, &needle)
                        || contains_ignore_case(&e.category, &needle)
                        || e.id.to_string() == needle
                })
                .map(|e| e.id)
                .collect();
            ui.weak(format!("{} match{}", matches.len(), if matches.len() == 1 { "" } else { "es" }));
            let row_height = ui.spacing().interact_size.y;
            ScrollArea::vertical().auto_shrink([false; 2]).show_rows(ui, row_height, matches.len(), |ui, range| {
                for &id in &matches[range] {
                    entity_label(ui, &mut ctx, id);
                }
            });
        }
        action
    }
}

struct Ctx<'a> {
    replay: &'a Replay,
    frame: i32,
    selected: &'a mut Option<i32>,
    hidden: &'a mut BTreeSet<i32>,
    action: &'a mut EntityAction,
}

fn node(ui: &mut egui::Ui, ctx: &mut Ctx, id: i32, depth: usize) {
    let children = ctx.replay.children_of(id);
    if children.is_empty() {
        ui.horizontal(|ui| {
            // Line up leaves with the label of a collapsible sibling, past its arrow.
            ui.add_space(ui.spacing().indent);
            entity_label(ui, ctx, id);
        });
        return;
    }

    let state_id = ui.make_persistent_id(("entity", id));
    CollapsingState::load_with_default_open(ui.ctx(), state_id, depth < 1)
        .show_header(ui, |ui| entity_label(ui, ctx, id))
        .body(|ui| {
            for &child in children {
                node(ui, ctx, child, depth + 1);
            }
        });
}

fn entity_label(ui: &mut egui::Ui, ctx: &mut Ctx, id: i32) {
    let Some(entity) = ctx.replay.entity(id) else {
        return;
    };
    let alive = ctx.replay.is_alive_at(id, ctx.frame);
    let is_hidden = ctx.hidden.contains(&id);
    let mut text = RichText::new(entity.display_name());
    if !alive {
        text = text.weak();
    }
    if is_hidden {
        text = text.strikethrough();
    }
    let response = ui.selectable_label(*ctx.selected == Some(id), text).on_hover_ui(|ui| {
        ui.label(format!("Id: {id}"));
        if !entity.path.is_empty() {
            ui.label(format!("Path: {}", entity.path));
        }
        if !entity.type_name.is_empty() {
            ui.label(format!("Type: {}", entity.type_name));
        }
        if !entity.category.is_empty() {
            ui.label(format!("Category: {}", entity.category));
        }
        if !alive {
            ui.weak("Not active at the current frame");
        }
        if is_hidden {
            ui.weak("Hidden in the viewport");
        }
    });
    if response.clicked() {
        *ctx.selected = Some(id);
    }
    if response.double_clicked() {
        ctx.action.focus = Some(id);
    }
    response.context_menu(|ui| {
        if ui.button("Focus in viewport").clicked() {
            ctx.action.focus = Some(id);
            ui.close();
        }
        let label = if is_hidden { "Show in viewport" } else { "Hide in viewport" };
        if ui.button(label).clicked() {
            if is_hidden {
                ctx.hidden.remove(&id);
            } else {
                ctx.hidden.insert(id);
            }
            ui.close();
        }
    });
}
