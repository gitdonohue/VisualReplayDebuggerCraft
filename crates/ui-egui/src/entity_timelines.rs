//! The entity timelines: one row per entity showing when it existed and when it logged something.

use std::collections::BTreeSet;

use egui::scroll_area::ScrollSource;
use egui::{Align2, Color32, FontId, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};
use vrd_engine::{Playback, TimeWindow};
use vrd_graph::{Lifespan, NameFilter, event_columns, frame_stripes, lifespans, span_columns};
use vrd_model::Replay;

use crate::timeaxis::{self, RULER_HEIGHT, TimeInput};
use crate::widgets::category_menu;

pub const ENTITY_ROW_HEIGHT: f32 = 20.0;
const GUTTER: f32 = 240.0;
const EYE_WIDTH: f32 = 22.0;
const MIN_STRIPE_PX: f32 = 4.0;

/// State of the entity timelines panel.
pub struct EntityTimelineView {
    pub filter: String,
    /// Leave out entities that never logged anything.
    pub logged_only: bool,
    hidden_categories: BTreeSet<String>,
    spans: Vec<Lifespan>,
    input: TimeInput,
    plot: Rect,
    shown: usize,
}

impl Default for EntityTimelineView {
    fn default() -> Self {
        Self {
            filter: String::new(),
            logged_only: false,
            hidden_categories: BTreeSet::new(),
            spans: Vec::new(),
            input: TimeInput::default(),
            plot: Rect::NOTHING,
            shown: 0,
        }
    }
}

fn name_of(replay: &Replay, entity: i32) -> String {
    replay.entity(entity).map_or_else(|| format!("#{entity}"), |e| e.display_name())
}

impl EntityTimelineView {
    pub fn set_replay(&mut self, replay: &Replay) {
        self.spans = lifespans(replay);
        self.hidden_categories.clear();
        self.input = TimeInput::default();
    }

    /// Where the bars were drawn last frame, in points.
    pub fn plot_rect(&self) -> Rect {
        self.plot
    }

    /// How many entity rows were listed last frame.
    pub fn shown(&self) -> usize {
        self.shown
    }

    fn visible(&self, replay: &Replay) -> Vec<usize> {
        let filter = NameFilter::new(&self.filter);
        self.spans
            .iter()
            .enumerate()
            .filter(|(_, span)| {
                let Some(entity) = replay.entity(span.entity) else {
                    return false;
                };
                (!self.logged_only || !span.events.is_empty())
                    && !self.hidden_categories.contains(&entity.category)
                    && (filter.is_empty() || filter.matches(&entity.display_name()) || filter.matches(&entity.path))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
        selected: &mut Option<i32>,
        hidden_entities: &mut BTreeSet<i32>,
    ) {
        self.toolbar(ui, replay, window);
        let rows = self.visible(replay);
        self.shown = rows.len();

        let width = ui.available_width();
        let (ruler_rect, ruler_response) = ui.allocate_exact_size(vec2(width, RULER_HEIGHT), Sense::click_and_drag());
        let gutter = GUTTER.min(width * 0.4);
        let plot_x = Rect::from_min_max(pos2(ruler_rect.left() + gutter, ruler_rect.top()), ruler_rect.right_bottom());
        if self.input.handle(ui, &ruler_response, plot_x, replay, window, playback) {
            window.fill();
        }

        ScrollArea::vertical()
            .id_salt("entity_timeline_rows")
            .auto_shrink([false; 2])
            .scroll_source(ScrollSource { scroll_bar: true, drag: false, mouse_wheel: false })
            .show_viewport(ui, |ui, viewport| {
                self.rows_ui(ui, viewport, plot_x, replay, window, playback, selected, hidden_entities, &rows);
            });

        let painter = ui.painter_at(ruler_rect);
        timeaxis::ruler(ui, &painter, Rect::from_min_max(plot_x.min, ruler_rect.right_bottom()), plot_x, None, window);
        painter.rect_filled(
            Rect::from_min_max(ruler_rect.min, pos2(plot_x.left(), ruler_rect.bottom())),
            0.0,
            ui.visuals().faint_bg_color,
        );
        timeaxis::playhead(&painter, window, plot_x, playback.time(), ruler_rect.top(), ruler_rect.bottom());
    }

    fn toolbar(&mut self, ui: &mut Ui, replay: &Replay, window: &mut TimeWindow) {
        ui.horizontal_wrapped(|ui| {
            ui.toggle_value(&mut window.follow, "Follow").on_hover_text("Keep the playhead in view");
            ui.toggle_value(&mut self.logged_only, "Logged only").on_hover_text("Only entities that logged something");
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter (a|b).").desired_width(140.0));
            if !self.filter.is_empty() && ui.small_button("x").on_hover_text("Clear filter").clicked() {
                self.filter.clear();
            }
            ui.menu_button("Entity categories", |ui| {
                let categories: Vec<String> = replay.entity_categories().iter().cloned().collect();
                category_menu(
                    ui,
                    "entity_timeline_categories",
                    categories.iter().map(|c| (c.as_str(), c)),
                    &mut self.hidden_categories,
                    "This capture has no entity categories.",
                );
            });
            ui.separator();
            if ui
                .add_enabled(!window.is_full(), egui::Button::new("Fit"))
                .on_hover_text("Show the whole capture (double-click empty space)")
                .clicked()
            {
                window.fill();
            }
            ui.add(
                egui::Label::new(egui::RichText::new(format!("{:.2}s - {:.2}s", window.start(), window.end())).weak())
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn rows_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        plot_x: Rect,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
        selected: &mut Option<i32>,
        hidden_entities: &mut BTreeSet<i32>,
        rows: &[usize],
    ) {
        let total = rows.len() as f32 * ENTITY_ROW_HEIGHT;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), total.max(viewport.height())), Sense::click_and_drag());
        let plot = Rect::from_min_max(pos2(plot_x.left(), rect.top()), pos2(plot_x.right(), rect.bottom()));
        self.plot = Rect::from_min_max(
            pos2(plot_x.left(), ui.clip_rect().top()),
            pos2(plot_x.right(), ui.clip_rect().bottom()),
        );
        let top = rect.top();
        let row_at = |y: f32| {
            let i = ((y - top) / ENTITY_ROW_HEIGHT).floor();
            (i >= 0.0).then_some(i as usize).filter(|&i| i < rows.len())
        };

        // The wheel zooms time over the bars and scrolls the rows over the names.
        let pointer = if self.input.is_dragging() { None } else { response.hover_pos() };
        if let Some(p) = pointer
            && p.x < plot.left()
        {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                ui.scroll_with_delta(vec2(0.0, delta));
            }
        }
        if self.input.handle(ui, &response, plot, replay, window, playback) {
            window.fill();
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos().filter(|p| p.x < plot.left())
            && let Some(i) = row_at(p.y)
        {
            let entity = self.spans[rows[i]].entity;
            if p.x < rect.left() + EYE_WIDTH {
                if !hidden_entities.remove(&entity) {
                    hidden_entities.insert(entity);
                }
            } else {
                *selected = Some(entity);
            }
        }

        let visuals = ui.visuals().clone();
        let dark = visuals.dark_mode;
        let vis = ui.clip_rect();
        let painter = ui.painter_at(vis);
        let (t0, t1) = (window.start(), window.end());
        painter.rect_filled(vis, 0.0, visuals.extreme_bg_color);
        let outside = if dark { Color32::from_gray(52) } else { Color32::from_gray(214) };
        let alive = if dark { Color32::from_rgb(70, 122, 160) } else { Color32::from_rgb(162, 208, 228) };
        let event = if dark { Color32::from_rgb(130, 180, 255) } else { Color32::from_rgb(20, 50, 210) };
        let stripe = if dark { Color32::from_white_alpha(10) } else { Color32::from_black_alpha(10) };

        let first = ((vis.top() - top) / ENTITY_ROW_HEIGHT).floor().max(0.0) as usize;
        let last = (((vis.bottom() - top) / ENTITY_ROW_HEIGHT).ceil().max(0.0) as usize).min(rows.len());
        for (position, &index) in rows.iter().enumerate().take(last).skip(first) {
            let span = &self.spans[index];
            let y = top + position as f32 * ENTITY_ROW_HEIGHT;
            let row = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), ENTITY_ROW_HEIGHT));
            let bar = Rect::from_min_max(pos2(plot.left(), y + 1.0), pos2(plot.right(), y + ENTITY_ROW_HEIGHT - 1.0));
            let is_selected = *selected == Some(span.entity);

            painter.rect_filled(
                Rect::from_min_max(row.left_top(), pos2(plot.left(), row.bottom())),
                0.0,
                if is_selected { visuals.selection.bg_fill.gamma_multiply(0.5) } else { visuals.faint_bg_color },
            );
            painter.rect_filled(bar, 0.0, outside);
            if let Some((x0, x1)) = span_columns(span.start, span.end, t0, t1, plot.width()) {
                let life = Rect::from_min_max(
                    pos2(bar.left() + x0, bar.top()),
                    pos2(bar.left() + x1.max(x0 + 1.0), bar.bottom()),
                );
                painter.rect_filled(life, 0.0, alive);
            }
            for x in event_columns(&span.events, t0, t1, plot.width()) {
                let x = bar.left() + x + 0.5;
                painter.line_segment([pos2(x, bar.top()), pos2(x, bar.bottom())], Stroke::new(1.0_f32, event));
            }
            if is_selected {
                painter.rect_stroke(
                    bar,
                    0.0,
                    Stroke::new(1.0_f32, visuals.selection.stroke.color),
                    egui::StrokeKind::Inside,
                );
            }

            let hidden = hidden_entities.contains(&span.entity);
            let eye = pos2(rect.left() + EYE_WIDTH * 0.5, row.center().y);
            if hidden {
                painter.circle_stroke(eye, 4.0, Stroke::new(1.0_f32, visuals.weak_text_color()));
            } else {
                painter.circle_filled(eye, 4.0, visuals.text_color());
            }
            let name_painter = painter.with_clip_rect(Rect::from_min_max(
                pos2(rect.left() + EYE_WIDTH, y),
                pos2(plot.left() - 2.0, y + ENTITY_ROW_HEIGHT),
            ));
            name_painter.text(
                pos2(rect.left() + EYE_WIDTH + 2.0, row.center().y),
                Align2::LEFT_CENTER,
                name_of(replay, span.entity),
                FontId::proportional(13.0),
                if hidden { visuals.weak_text_color() } else { visuals.text_color() },
            );
        }

        for (x0, x1) in frame_stripes(replay.frame_times(), t0, t1, plot.width(), MIN_STRIPE_PX) {
            painter.rect_filled(
                Rect::from_min_max(pos2(plot.left() + x0, vis.top()), pos2(plot.left() + x1, vis.bottom())),
                0.0,
                stripe,
            );
        }
        timeaxis::playhead(&painter, window, plot, playback.time(), vis.top(), vis.bottom());

        if rows.is_empty() {
            painter.text(
                pos2(rect.left() + 8.0, top + 12.0),
                Align2::LEFT_CENTER,
                "No entities match.",
                FontId::proportional(13.0),
                visuals.weak_text_color(),
            );
        }
        if let Some(p) = pointer.filter(|p| p.x >= plot.left())
            && let Some(i) = row_at(p.y)
        {
            let span = &self.spans[rows[i]];
            response.on_hover_text(format!(
                "{}\n{:.2}s to {:.2}s, {} log entries",
                name_of(replay, span.entity),
                span.start,
                span.end,
                span.events.len()
            ));
        }
    }
}
