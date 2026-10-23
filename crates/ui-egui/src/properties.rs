use egui::{Grid, RichText, ScrollArea};
use vrd_model::Replay;

/// The properties of one entity as of the current frame.
pub fn ui(ui: &mut egui::Ui, replay: &Replay, selected: Option<i32>, frame: i32) {
    let Some(id) = selected else {
        ui.weak("Select an entity to see its properties.");
        return;
    };
    let Some(entity) = replay.entity(id) else {
        ui.weak("The selected entity does not exist in this capture.");
        return;
    };

    ui.heading(entity.display_name());
    let subtitle: Vec<&str> =
        [entity.type_name.as_str(), entity.category.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
    if !subtitle.is_empty() {
        ui.weak(subtitle.join(" · "));
    }
    if let Some((start, end)) = replay.lifetime(id) {
        let alive = replay.is_alive_at(id, frame);
        ui.label(
            RichText::new(format!("Frames {start} to {end}{}", if alive { "" } else { " (not active now)" })).weak(),
        );
    }
    ui.separator();

    ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        Grid::new(("properties", id)).num_columns(2).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            for (key, value) in replay.properties_at(id, frame) {
                ui.label(RichText::new(key).strong());
                ui.add(egui::Label::new(value).wrap().selectable(true));
                ui.end_row();
            }
        });
    });
}
