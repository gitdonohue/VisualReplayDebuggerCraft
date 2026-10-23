use egui::{Button, Slider};
use vrd_engine::{MAX_SPEED, MIN_SPEED, Playback};
use vrd_model::Replay;

/// Play/pause, stepping, speed and the time scrubber.
pub fn ui(ui: &mut egui::Ui, replay: &Replay, playback: &mut Playback) {
    ui.horizontal(|ui| {
        if ui.button("⏮").on_hover_text("Rewind (Home)").clicked() {
            playback.rewind(replay);
        }
        if ui.button("⏪").on_hover_text("Previous log entry (Page Up)").clicked() {
            playback.step_to_previous_log(replay);
        }
        if ui.button("◀").on_hover_text("Step back (Left)").clicked() {
            playback.step(replay, -1);
        }
        let (icon, tip) = if playback.is_playing() { ("⏸", "Pause (Space)") } else { ("▶", "Play (Space)") };
        if ui.add(Button::new(icon).min_size(egui::vec2(32.0, 0.0))).on_hover_text(tip).clicked() {
            playback.toggle(replay);
        }
        if ui.button("▶|").on_hover_text("Step forward (Right)").clicked() {
            playback.step(replay, 1);
        }
        if ui.button("⏩").on_hover_text("Next log entry (Page Down)").clicked() {
            playback.step_to_next_log(replay);
        }
        if ui.button("⏭").on_hover_text("Go to end (End)").clicked() {
            playback.pause();
            playback.seek_frame(replay, replay.last_frame());
        }

        ui.separator();

        let mut speed = playback.speed();
        if ui
            .add(egui::DragValue::new(&mut speed).speed(0.02).range(MIN_SPEED..=MAX_SPEED).max_decimals(2).suffix("×"))
            .on_hover_text("Playback speed")
            .changed()
        {
            playback.set_speed(speed);
        }

        let mut looping = playback.is_looping();
        if ui.checkbox(&mut looping, "Loop").changed() {
            playback.set_looping(looping);
        }

        ui.separator();
        ui.monospace(format!(
            "Frame {} / {}    {:.3} s / {:.3} s",
            playback.frame(),
            replay.last_frame(),
            playback.time(),
            replay.total_time()
        ));
    });

    let total = replay.total_time();
    ui.add_enabled_ui(total > 0.0, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(50.0);
        let mut t = playback.time();
        let slider = Slider::new(&mut t, 0.0..=total.max(f64::EPSILON)).show_value(false).trailing_fill(true);
        if ui.add(slider).changed() {
            playback.seek_time(replay, t);
        }
    });
}
