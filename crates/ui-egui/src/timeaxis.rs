//! The time axis the graphs and the parameter timelines share: the ruler, and the mouse handling
//! that moves the playhead and the view.

use egui::{Align2, Color32, FontId, Painter, Rect, Response, Stroke, Ui, pos2};
use vrd_engine::{Playback, TimeWindow};
use vrd_graph::{format_time, time_ticks};
use vrd_model::Replay;

pub const RULER_HEIGHT: f32 = 18.0;
/// How close to the playhead a drag must start to move the playhead rather than the view.
const SCRUB_GRAB: f32 = 8.0;
pub const PLAYHEAD: Color32 = Color32::from_rgb(230, 40, 40);

/// Where a time falls on screen, given the span `plot` the window is drawn across.
pub fn x_of(window: &TimeWindow, plot: Rect, t: f64) -> f32 {
    plot.left() + window.fraction_of(t) as f32 * plot.width()
}

/// Tick marks and labels in `ruler`, with faint guide lines down through `grid`.
pub fn ruler(ui: &Ui, painter: &Painter, ruler: Rect, plot: Rect, grid: Option<Rect>, window: &TimeWindow) {
    let visuals = ui.visuals();
    painter.rect_filled(ruler, 0.0, visuals.faint_bg_color);
    let (step, ticks) = time_ticks(window.start(), window.end(), plot.width(), 70.0);
    let weak = visuals.weak_text_color();
    let guide = weak.gamma_multiply(0.18);
    for t in ticks {
        let x = x_of(window, plot, t);
        painter.line_segment([pos2(x, ruler.bottom() - 5.0), pos2(x, ruler.bottom())], Stroke::new(1.0_f32, weak));
        if let Some(grid) = grid {
            painter.line_segment([pos2(x, grid.top()), pos2(x, grid.bottom())], Stroke::new(1.0_f32, guide));
        }
        painter.text(
            pos2(x + 3.0, ruler.center().y),
            Align2::LEFT_CENTER,
            format_time(t, step),
            FontId::proportional(11.0),
            weak,
        );
    }
    painter.line_segment([ruler.left_bottom(), ruler.right_bottom()], visuals.widgets.noninteractive.bg_stroke);
}

/// The playhead as a line from `top` to `bottom`.
pub fn playhead(painter: &Painter, window: &TimeWindow, plot: Rect, t: f64, top: f32, bottom: f32) {
    if t >= window.start() && t <= window.end() {
        let x = x_of(window, plot, t);
        painter.line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(1.5_f32, PLAYHEAD));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    Scrub,
    Pan,
}

/// Mouse handling for a view of the time window: click to seek, drag the playhead to scrub, drag
/// elsewhere to pan, scroll to zoom about the pointer, shift-scroll or sideways scroll to pan.
#[derive(Debug, Default)]
pub struct TimeInput {
    drag: Option<Drag>,
}

impl TimeInput {
    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Handles `response`, whose pointer positions are mapped to time by their x within `plot`.
    /// Presses and scrolling outside `plot`'s horizontal span are ignored. Returns whether the view
    /// was double-clicked there, for the caller to decide what that means.
    pub fn handle(
        &mut self,
        ui: &Ui,
        response: &Response,
        plot: Rect,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
    ) -> bool {
        let width = plot.width().max(1.0);
        let fraction = |x: f32| f64::from(((x - plot.left()) / width).clamp(0.0, 1.0));
        let in_plot = |x: f32| x >= plot.left() && x <= plot.right();

        if response.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin()).filter(|p| in_plot(p.x))
        {
            // Decide by where the press began: egui reports the drag only after the pointer moved.
            let playhead_x = x_of(window, plot, playback.time());
            self.drag = Some(if (p.x - playhead_x).abs() <= SCRUB_GRAB { Drag::Scrub } else { Drag::Pan });
        }
        if response.dragged() {
            match self.drag {
                Some(Drag::Scrub) => {
                    if let Some(p) = response.interact_pointer_pos() {
                        playback.seek_time(replay, window.time_at(fraction(p.x)));
                    }
                }
                Some(Drag::Pan) => window.pan(-f64::from(response.drag_delta().x / width) * window.range()),
                None => {}
            }
        }
        if response.drag_stopped() {
            self.drag = None;
        }

        let mut double_clicked = false;
        if let Some(p) = response.interact_pointer_pos().filter(|p| in_plot(p.x)) {
            if response.clicked() {
                playback.seek_time(replay, window.time_at(fraction(p.x)));
            }
            double_clicked = response.double_clicked();
        }

        if response.hovered()
            && let Some(p) = response.hover_pos().filter(|p| in_plot(p.x))
        {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            let about = window.time_at(fraction(p.x));
            if scroll.y != 0.0 {
                window.zoom((-f64::from(scroll.y) * 0.003).exp(), about);
            }
            if zoom != 1.0 {
                window.zoom(1.0 / f64::from(zoom), about);
            }
            if scroll.x != 0.0 {
                window.pan(-f64::from(scroll.x / width) * window.range());
            }
        }
        double_clicked
    }
}

/// Zooms the window to `start..end` with a little room around it.
pub fn zoom_to(window: &mut TimeWindow, start: f64, end: f64) {
    let pad = (end - start).max(0.0) * 0.1;
    window.set(start - pad, end + pad);
}
