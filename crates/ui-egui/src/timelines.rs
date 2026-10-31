//! The parameter timelines panel: what the string parameters of the selected and pinned entities
//! were over time, as rows of blocks.

use std::collections::BTreeSet;

use egui::scroll_area::ScrollSource;
use egui::{Align2, Color32, FontId, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use vrd_engine::{Playback, TimeWindow};
use vrd_graph::{
    Block, BlockKind, Level, NameFilter, RowOptions, Section, TimelineData, blocks, event_times, frame_stripes,
    neighbouring_event, row_level, rows, smallest_block_at,
};
use vrd_model::Replay;

use crate::pins::Pins;
use crate::timeaxis::{self, RULER_HEIGHT, TimeInput, x_of, zoom_to};
use crate::widgets::{category_menu, label_colors, readout};

/// Height of one row of blocks, in points.
pub const ROW_HEIGHT: f32 = 20.0;
/// Height of the line naming an entity.
pub const HEADER_HEIGHT: f32 = 20.0;
const GUTTER: f32 = 200.0;
/// Frames are told apart by shading when each is at least this many points wide.
const MIN_STRIPE_PX: f32 = 4.0;
const MIN_LABEL_PX: f32 = 18.0;

/// What the toolbar asked for that needs the rows to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nav {
    PreviousEvent,
    NextEvent,
    ZoomBlock,
}

/// A block under the pointer.
#[derive(Debug, Clone, Copy)]
struct Hit {
    section: usize,
    row: usize,
    block: Block,
}

/// State of the parameter timelines panel.
pub struct TimelineView {
    /// One row per level of the values' paths. Off: one row per parameter.
    pub by_depth: bool,
    /// Leave out rows whose value never changes.
    pub changes_only: bool,
    pub filter: String,
    hidden: BTreeSet<String>,
    data: TimelineData,
    input: TimeInput,
    plot: Rect,
    shown: (usize, usize),
}

impl Default for TimelineView {
    fn default() -> Self {
        Self {
            by_depth: true,
            changes_only: false,
            filter: String::new(),
            hidden: BTreeSet::new(),
            data: TimelineData::default(),
            input: TimeInput::default(),
            plot: Rect::NOTHING,
            shown: (0, 0),
        }
    }
}

impl TimelineView {
    pub fn set_replay(&mut self, replay: &Replay) {
        self.data = TimelineData::build(replay);
        self.hidden.clear();
        self.input = TimeInput::default();
    }

    /// Whether an entity recorded any text parameters worth showing.
    pub fn has_parameters(&self, entity: i32) -> bool {
        self.data.has_tracks(entity)
    }

    /// The area the blocks are drawn in, as last seen: the horizontal span of the time axis, and the
    /// part of the rows that is in view. Its top is where the first entity's header starts.
    pub fn plot_rect(&self) -> Rect {
        self.plot
    }

    /// How many entities and rows were shown last frame.
    pub fn shown(&self) -> (usize, usize) {
        self.shown
    }

    /// The time of the nearest start or stop of a block before or after the playhead, among the rows
    /// shown for `selected` and `pins`.
    pub fn neighbouring_event(&self, selected: Option<i32>, pins: &Pins, t: f64, forward: bool) -> Option<f64> {
        let entities = pins.shown(selected, |id| self.data.has_tracks(id));
        let sections = self.sections(&entities);
        let levels = sections.iter().flat_map(|s| &s.rows).map(|r| row_level(&self.data, &entities, r));
        neighbouring_event(&event_times(levels), t, forward)
    }

    fn sections(&self, entities: &[i32]) -> Vec<Section> {
        let filter = NameFilter::new(&self.filter);
        rows(
            &self.data,
            entities,
            &RowOptions {
                by_depth: self.by_depth,
                changes_only: self.changes_only,
                filter: &filter,
                hidden: &self.hidden,
            },
        )
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
        selected: &mut Option<i32>,
        pins: &mut Pins,
    ) {
        let entities = pins.shown(*selected, |id| self.data.has_tracks(id));
        let sections = self.sections(&entities);

        if let Some(nav) = self.toolbar(ui, window, *selected, pins, &entities) {
            self.navigate(nav, replay, window, playback, &entities, &sections);
        }

        self.shown = (sections.len(), sections.iter().map(|s| s.rows.len()).sum());
        if sections.is_empty() {
            self.plot = Rect::NOTHING;
            self.shown = (0, 0);
            if entities.is_empty() {
                self.hint(ui, replay, selected);
            } else {
                ui.add_space(6.0);
                ui.weak("No parameters match.");
            }
            return;
        }

        // The ruler stays put while the rows scroll under it.
        let width = ui.available_width();
        let (ruler_rect, ruler_response) = ui.allocate_exact_size(vec2(width, RULER_HEIGHT), Sense::click_and_drag());
        let gutter = GUTTER.min(width * 0.4);
        let plot_x = Rect::from_min_max(pos2(ruler_rect.left() + gutter, ruler_rect.top()), ruler_rect.right_bottom());
        if self.input.handle(ui, &ruler_response, plot_x, replay, window, playback) {
            window.fill();
        }

        ScrollArea::vertical()
            .id_salt("timeline_rows")
            .auto_shrink([false; 2])
            .scroll_source(ScrollSource { scroll_bar: true, drag: false, mouse_wheel: false })
            .show_viewport(ui, |ui, viewport| {
                self.rows_ui(ui, viewport, plot_x, replay, window, playback, &entities, &sections, pins);
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

    fn toolbar(
        &mut self,
        ui: &mut Ui,
        window: &mut TimeWindow,
        selected: Option<i32>,
        pins: &mut Pins,
        entities: &[i32],
    ) -> Option<Nav> {
        let mut nav = None;
        ui.horizontal_wrapped(|ui| {
            ui.toggle_value(&mut self.by_depth, "By depth")
                .on_hover_text("One row per level of the value (A.B.C). Off: one row per parameter");
            ui.toggle_value(&mut self.changes_only, "Changes only")
                .on_hover_text("Hide rows whose value never changes");
            ui.toggle_value(&mut window.follow, "Follow").on_hover_text("Keep the playhead in view");
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter (a|b)…").desired_width(140.0));
            if !self.filter.is_empty() && ui.small_button("✕").on_hover_text("Clear filter").clicked() {
                self.filter.clear();
            }
            ui.menu_button("Parameters", |ui| {
                let names: BTreeSet<String> =
                    entities.iter().flat_map(|&e| self.data.tracks(e)).map(|t| t.name.clone()).collect();
                let names: Vec<String> = names.into_iter().collect();
                category_menu(
                    ui,
                    "timeline_parameters",
                    names.iter().map(|n| (n.as_str(), n)),
                    &mut self.hidden,
                    "Nothing to choose from.",
                );
            });
            ui.separator();
            let any = !entities.is_empty();
            if ui.add_enabled(any, egui::Button::new("◀ Event")).on_hover_text("Previous change on screen").clicked()
            {
                nav = Some(Nav::PreviousEvent);
            }
            if ui.add_enabled(any, egui::Button::new("Event ▶")).on_hover_text("Next change on screen").clicked() {
                nav = Some(Nav::NextEvent);
            }
            if ui
                .add_enabled(any, egui::Button::new("Zoom block"))
                .on_hover_text("Zoom to the shortest block under the playhead (double-click a block)")
                .clicked()
            {
                nav = Some(Nav::ZoomBlock);
            }
            ui.separator();
            if let Some(id) = selected.filter(|&id| self.data.has_tracks(id)) {
                let label = if pins.contains(id) { "Unpin selection" } else { "Pin selection" };
                if ui.button(label).on_hover_text("Keep this entity shown while selecting others").clicked() {
                    pins.toggle(id);
                }
            }
            if !pins.is_empty() && ui.button("Clear pins").clicked() {
                pins.clear();
            }
            if ui
                .add_enabled(!window.is_full(), egui::Button::new("Fit"))
                .on_hover_text("Show the whole capture (double-click empty space)")
                .clicked()
            {
                window.fill();
            }
            ui.add(
                egui::Label::new(egui::RichText::new(format!("{:.2}s – {:.2}s", window.start(), window.end())).weak())
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
        nav
    }

    fn navigate(
        &self,
        nav: Nav,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
        entities: &[i32],
        sections: &[Section],
    ) {
        let levels = || sections.iter().flat_map(|s| &s.rows).map(|r| row_level(&self.data, entities, r));
        match nav {
            Nav::PreviousEvent | Nav::NextEvent => {
                let events = event_times(levels());
                if let Some(t) = neighbouring_event(&events, playback.time(), nav == Nav::NextEvent) {
                    playback.seek_time(replay, t);
                }
            }
            Nav::ZoomBlock => {
                if let Some((start, end)) = smallest_block_at(levels(), playback.time()) {
                    zoom_to(window, start, end);
                }
            }
        }
    }

    fn hint(&self, ui: &mut Ui, replay: &Replay, selected: &mut Option<i32>) {
        ui.add_space(6.0);
        ui.weak(if selected.is_some() {
            "The selected entity recorded no text parameters."
        } else {
            "Select an entity to see its text parameters over time."
        });
        if self.data.entities().next().is_none() {
            ui.weak("This capture has no text parameters at all.");
            return;
        }
        ui.label("Entities with text parameters:");
        ScrollArea::vertical().id_salt("timeline_hint").auto_shrink([false; 2]).show(ui, |ui| {
            for id in self.data.entities() {
                let name = replay.entity(id).map_or_else(|| format!("#{id}"), |e| e.display_name());
                let n = self.data.tracks(id).len();
                if ui.link(format!("{name} ({n} parameters)")).clicked() {
                    *selected = Some(id);
                }
            }
        });
    }

    /// The block at `p`, if the pointer is on one.
    fn block_at(
        &self,
        sections: &[Section],
        entities: &[i32],
        top: f32,
        plot: Rect,
        window: &TimeWindow,
        p: egui::Pos2,
    ) -> Option<Hit> {
        if p.x < plot.left() || p.x > plot.right() {
            return None;
        }
        let (section, row) = row_at(sections, p.y - top)?;
        let level = row_level(&self.data, entities, &sections[section].rows[row]);
        let x = p.x - plot.left();
        blocks(&level.runs, window.start(), window.end(), plot.width())
            .into_iter()
            .find(|b| x >= b.x0 - 1.5 && x <= b.x1.max(b.x0 + 1.0) + 1.5)
            .map(|block| Hit { section, row, block })
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
        entities: &[i32],
        sections: &[Section],
        pins: &mut Pins,
    ) {
        let total: f32 = sections.iter().map(|s| HEADER_HEIGHT + s.rows.len() as f32 * ROW_HEIGHT).sum();
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), total.max(viewport.height())), Sense::click_and_drag());
        let plot = Rect::from_min_max(pos2(plot_x.left(), rect.top()), pos2(plot_x.right(), rect.bottom()));
        self.plot = Rect::from_min_max(
            pos2(plot_x.left(), ui.clip_rect().top()),
            pos2(plot_x.right(), ui.clip_rect().bottom()),
        );
        // Where the first header starts in screen space, whatever the scroll position.
        let top = rect.top();

        // The wheel zooms time over the blocks and scrolls the rows over the labels.
        let pointer = if self.input.is_dragging() { None } else { response.hover_pos() };
        if let Some(p) = pointer
            && p.x < plot.left()
        {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                ui.scroll_with_delta(vec2(0.0, delta));
            }
        }

        let hit = pointer.and_then(|p| self.block_at(sections, entities, top, plot, window, p));
        if self.input.handle(ui, &response, plot, replay, window, playback) {
            match hit {
                Some(h) => zoom_to(window, h.block.start, h.block.end),
                None => window.fill(),
            }
        }

        let visuals = ui.visuals().clone();
        let dark = visuals.dark_mode;
        let vis = ui.clip_rect();
        let painter = ui.painter_at(vis);
        let (t0, t1) = (window.start(), window.end());
        painter.rect_filled(vis, 0.0, visuals.extreme_bg_color);
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), vis.top()), pos2(plot.left(), vis.bottom())),
            0.0,
            visuals.faint_bg_color,
        );
        let stripe = if dark { Color32::from_white_alpha(12) } else { Color32::from_black_alpha(12) };
        for (x0, x1) in frame_stripes(replay.frame_times(), t0, t1, plot.width(), MIN_STRIPE_PX) {
            painter.rect_filled(
                Rect::from_min_max(pos2(plot.left() + x0, vis.top()), pos2(plot.left() + x1, vis.bottom())),
                0.0,
                stripe,
            );
        }

        let weak = visuals.weak_text_color();
        let line = weak.gamma_multiply(0.3);
        // Before an entity exists and after it is gone: darker in either theme.
        let shade = if dark { Color32::from_black_alpha(90) } else { Color32::from_black_alpha(28) };
        let mut toggled = None;
        let mut y = top;
        for (si, section) in sections.iter().enumerate() {
            let entity = entities[section.slot];
            let header = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), HEADER_HEIGHT));
            y += HEADER_HEIGHT;
            if header.intersects(vis) {
                painter.rect_filled(header, 0.0, visuals.faint_bg_color.gamma_multiply(1.6));
                if si > 0 {
                    painter.line_segment(
                        [header.left_top(), header.right_top()],
                        Stroke::new(1.5_f32, weak.gamma_multiply(0.6)),
                    );
                }
                let galley = painter.layout_no_wrap(
                    entity_name(replay, entity),
                    FontId::proportional(13.0),
                    visuals.strong_text_color(),
                );
                let width = galley.size().x;
                painter.galley(
                    pos2(header.left() + 6.0, header.center().y - galley.size().y * 0.5),
                    galley,
                    visuals.strong_text_color(),
                );
                let pinned = pins.contains(entity);
                let button =
                    egui::Button::new(RichText::new(if pinned { "Unpin" } else { "Pin" }).small()).selected(pinned);
                let button_rect = Rect::from_min_size(
                    pos2(header.left() + 14.0 + width, header.top() + 2.0),
                    vec2(42.0, HEADER_HEIGHT - 4.0),
                );
                if ui.put(button_rect, button).on_hover_text("Keep showing this entity whatever is selected").clicked()
                {
                    toggled = Some(entity);
                }
            }

            let tracks = self.data.tracks(entity);
            for (ri, row) in section.rows.iter().enumerate() {
                let row_rect = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), ROW_HEIGHT));
                y += ROW_HEIGHT;
                if !row_rect.intersects(vis) {
                    continue;
                }
                let track = &tracks[row.track];
                let level = track.level(row.level);

                // Before the entity existed and after it is gone there is nothing to show.
                let (from, to) = (x_of(window, plot, track.valid.0), x_of(window, plot, track.valid.1));
                if from > plot.left() {
                    painter.rect_filled(
                        Rect::from_min_max(
                            row_rect.left_top().max(pos2(plot.left(), row_rect.top())),
                            pos2(from.min(plot.right()), row_rect.bottom()),
                        ),
                        0.0,
                        shade,
                    );
                }
                if to < plot.right() {
                    painter.rect_filled(
                        Rect::from_min_max(pos2(to.max(plot.left()), row_rect.top()), row_rect.right_bottom()),
                        0.0,
                        shade,
                    );
                }

                if row.first {
                    let clip = Rect::from_min_max(row_rect.left_top(), pos2(plot.left() - 4.0, row_rect.bottom()));
                    painter.with_clip_rect(clip).text(
                        pos2(row_rect.left() + 14.0, row_rect.center().y),
                        Align2::LEFT_CENTER,
                        &track.short_name,
                        FontId::proportional(11.0),
                        visuals.text_color(),
                    );
                }
                let last = section.rows.get(ri + 1).is_none_or(|next| next.first);
                painter.line_segment(
                    [row_rect.left_bottom(), row_rect.right_bottom()],
                    Stroke::new(1.0_f32, if last { weak.gamma_multiply(0.5) } else { line }),
                );

                let hovered = hit.filter(|h| h.section == si && h.row == ri);
                for block in blocks(&level.runs, t0, t1, plot.width()) {
                    let is_hovered = hovered.is_some_and(|h| h.block.start == block.start && h.block.end == block.end);
                    self.paint_block(&painter, row_rect, plot, level, &block, is_hovered, dark);
                }
            }
        }
        // A fill to the bottom, so that short lists do not leave stale pixels under the last row.
        painter
            .line_segment([pos2(plot.left(), vis.top()), pos2(plot.left(), vis.bottom())], Stroke::new(1.0_f32, line));
        timeaxis::playhead(&painter, window, plot, playback.time(), vis.top(), vis.bottom());

        if let Some(p) = pointer.filter(|p| vis.contains(*p)) {
            self.readout(ui, &painter, replay, vis, plot, window, entities, sections, top, hit, p);
        }
        if let Some(entity) = toggled {
            pins.toggle(entity);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_block(
        &self,
        painter: &egui::Painter,
        row: Rect,
        plot: Rect,
        level: &Level,
        block: &Block,
        hovered: bool,
        dark: bool,
    ) {
        let rect = Rect::from_min_max(
            pos2(plot.left() + block.x0, row.top() + 1.5),
            pos2(plot.left() + block.x1.max(block.x0 + 1.0), row.bottom() - 1.5),
        );
        let (key, text) = match block.kind {
            BlockKind::Value { key } => (key, None),
            BlockKind::Busy { changes, key } => (key, Some(format!("{changes}×"))),
        };
        let label = &level.labels[key as usize];
        let (fill, ink) = label_colors(self.data.color_index(label), dark);
        let busy = text.is_some();
        // Busy blocks are drawn dimmer and with an inked edge, so they read as "many things here".
        let fill = if busy { blend(fill, ink, 0.35) } else { fill };
        let stroke = if hovered {
            Stroke::new(2.0_f32, Color32::WHITE)
        } else {
            Stroke::new(if busy { 1.0_f32 } else { 0.75_f32 }, ink)
        };
        painter.rect(rect, 2.0, fill, stroke, StrokeKind::Inside);

        let shown = text.as_deref().unwrap_or(label);
        let wide_enough = rect.width() >= if busy { 26.0 } else { MIN_LABEL_PX };
        if wide_enough && !shown.is_empty() {
            painter.with_clip_rect(rect.shrink2(vec2(1.0, 0.0))).text(
                pos2(rect.left() + 3.0, rect.center().y),
                Align2::LEFT_CENTER,
                shown,
                FontId::proportional(11.0),
                ink,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn readout(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        replay: &Replay,
        vis: Rect,
        plot: Rect,
        window: &TimeWindow,
        entities: &[i32],
        sections: &[Section],
        top: f32,
        hit: Option<Hit>,
        p: egui::Pos2,
    ) {
        let visuals = ui.visuals();
        let strong = visuals.strong_text_color();
        // Over the labels: the full name of the parameter.
        if p.x < plot.left() {
            if let Some((si, ri)) = row_at(sections, p.y - top) {
                let row = &sections[si].rows[ri];
                if row.first {
                    let entity = entities[sections[si].slot];
                    let track = &self.data.tracks(entity)[row.track];
                    let lines =
                        vec![(entity_name(replay, entity), strong), (truncate(&track.name, 120), visuals.text_color())];
                    readout(ui, painter, vis, p, lines);
                }
            }
            return;
        }

        let Some(hit) = hit else {
            return;
        };
        let section = &sections[hit.section];
        let row = &section.rows[hit.row];
        let entity = entities[section.slot];
        let track = &self.data.tracks(entity)[row.track];
        let level = track.level(row.level);
        let t = window.time_at(f64::from((p.x - plot.left()) / plot.width().max(1.0)));
        let dark = visuals.dark_mode;
        let b = hit.block;

        let mut lines = vec![(format!("{} · {}", entity_name(replay, entity), truncate(&track.name, 100)), strong)];
        match b.kind {
            BlockKind::Value { key } => {
                let (_, ink) = label_colors(self.data.color_index(&level.labels[key as usize]), dark);
                lines.push((truncate(&level.paths[key as usize], 120), ink));
                if let Some(value) = track.value_at(t).filter(|v| *v != level.paths[key as usize]) {
                    lines.push((format!("value: {}", truncate(value, 120)), visuals.text_color()));
                }
                lines.push((
                    format!("{:.3}s – {:.3}s  ({:.3}s)", b.start, b.end, b.end - b.start),
                    visuals.weak_text_color(),
                ));
            }
            BlockKind::Busy { changes, key } => {
                let (_, ink) = label_colors(self.data.color_index(&level.labels[key as usize]), dark);
                if b.end - b.start <= f64::EPSILON {
                    lines.push((format!("changed at {:.3}s", b.start), ink));
                } else {
                    lines.push((format!("{changes} changes, mostly {}", level.labels[key as usize]), ink));
                    lines.push((format!("{:.3}s – {:.3}s", b.start, b.end), visuals.weak_text_color()));
                    lines.push(("Double-click to zoom in".to_string(), visuals.weak_text_color()));
                }
            }
        }
        readout(ui, painter, vis, p, lines);
    }
}

/// The row at `offset` points below the top of the first header: section and row within it.
/// `None` over a header or outside the rows.
fn row_at(sections: &[Section], offset: f32) -> Option<(usize, usize)> {
    if offset < 0.0 {
        return None;
    }
    let mut y = 0.0;
    for (si, section) in sections.iter().enumerate() {
        y += HEADER_HEIGHT;
        let rows_end = y + section.rows.len() as f32 * ROW_HEIGHT;
        if offset < y {
            return None;
        }
        if offset < rows_end {
            return Some((si, ((offset - y) / ROW_HEIGHT) as usize));
        }
        y = rows_end;
    }
    None
}

fn entity_name(replay: &Replay, id: i32) -> String {
    replay.entity(id).map_or_else(|| format!("#{id}"), |e| e.display_name())
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) * (1.0 - t) + f32::from(y) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrd_graph::Row;

    fn section(slot: usize, rows: usize) -> Section {
        Section { slot, rows: (0..rows).map(|i| Row { slot, track: 0, level: Some(i), first: i == 0 }).collect() }
    }

    #[test]
    fn pointer_offsets_map_to_rows() {
        let sections = [section(0, 2), section(1, 1)];
        let (h, r) = (HEADER_HEIGHT, ROW_HEIGHT);
        assert_eq!(row_at(&sections, -1.0), None);
        assert_eq!(row_at(&sections, h * 0.5), None, "on the first header");
        assert_eq!(row_at(&sections, h + 1.0), Some((0, 0)));
        assert_eq!(row_at(&sections, h + r + 1.0), Some((0, 1)));
        assert_eq!(row_at(&sections, h + 2.0 * r + 1.0), None, "on the second header");
        assert_eq!(row_at(&sections, 2.0 * h + 2.0 * r + 1.0), Some((1, 0)));
        assert_eq!(row_at(&sections, 2.0 * h + 3.0 * r + 5.0), None, "below the last row");
        assert_eq!(row_at(&[], 5.0), None);
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 4), "abcd…");
        assert_eq!(truncate("héllo wörld", 5), "héllo…");
    }

    #[test]
    fn blending_endpoints() {
        let (a, b) = (Color32::from_rgb(0, 0, 0), Color32::from_rgb(200, 100, 50));
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color32::from_rgb(100, 50, 25));
    }
}
