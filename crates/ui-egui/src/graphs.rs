//! The graphs panel: recorded numbers over time for the selected and pinned entities.

use egui::{Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, pos2, vec2};
use vrd_engine::{Playback, TimeWindow};
use vrd_graph::{Band, GraphData, Layout, Member, NameFilter, Scale, Series, Stacking, format_value, layout, trace};
use vrd_model::Replay;

use crate::pins::Pins;
use crate::timeaxis::{self, RULER_HEIGHT, TimeInput};
use crate::widgets::{readout, tag};

const HEADER_HEIGHT: f32 = 18.0;
/// Bands with more series than this get no labels at the playhead (hover for values instead).
const MAX_PLAYHEAD_LABELS: usize = 8;

/// Tableau 10: distinct on both light and dark backgrounds.
const PALETTE: [Color32; 10] = [
    Color32::from_rgb(31, 119, 180),
    Color32::from_rgb(255, 127, 14),
    Color32::from_rgb(44, 160, 44),
    Color32::from_rgb(214, 39, 40),
    Color32::from_rgb(148, 103, 189),
    Color32::from_rgb(140, 86, 75),
    Color32::from_rgb(227, 119, 194),
    Color32::from_rgb(127, 127, 127),
    Color32::from_rgb(188, 189, 34),
    Color32::from_rgb(23, 190, 207),
];

/// State of the graphs panel.
pub struct GraphView {
    pub stacking: Stacking,
    pub filled: bool,
    pub ignore_outliers: bool,
    pub filter: String,
    data: GraphData,
    input: TimeInput,
    rect: Rect,
    shown: (usize, usize),
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            stacking: Stacking::default(),
            filled: true,
            ignore_outliers: false,
            filter: String::new(),
            data: GraphData::default(),
            input: TimeInput::default(),
            rect: Rect::NOTHING,
            shown: (0, 0),
        }
    }
}

impl GraphView {
    pub fn set_replay(&mut self, replay: &Replay) {
        self.data = GraphData::build(replay);
        self.input = TimeInput::default();
    }

    pub fn has_values(&self, entity: i32) -> bool {
        self.data.has_series(entity)
    }

    /// Where the graphs were last drawn.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// How many entities and series were drawn last frame.
    pub fn shown(&self) -> (usize, usize) {
        self.shown
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        replay: &Replay,
        window: &mut TimeWindow,
        playback: &mut Playback,
        selected: &mut Option<i32>,
        pins: &mut Pins,
    ) {
        self.toolbar(ui, window, *selected, pins);

        let graphed = pins.shown(*selected, |id| self.data.has_series(id));
        if graphed.is_empty() {
            self.rect = Rect::NOTHING;
            self.shown = (0, 0);
            self.hint(ui, replay, selected);
            return;
        }

        let filter = NameFilter::new(&self.filter);
        let lay = {
            let lists: Vec<Vec<(usize, &str, usize)>> = graphed
                .iter()
                .map(|&id| {
                    self.data
                        .series(id)
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| filter.matches(&s.name))
                        .map(|(i, s)| (i, s.name.as_str(), s.depth))
                        .collect()
                })
                .collect();
            layout(&lists, self.stacking)
        };

        let size = ui.available_size().max(vec2(64.0, 48.0));
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        self.rect = rect;
        let area = Rect::from_min_max(pos2(rect.left(), rect.top() + RULER_HEIGHT), rect.max);

        if self.input.handle(ui, &response, area, replay, window, playback) {
            window.fill();
        }

        let painter = ui.painter_at(rect);
        let view = View {
            data: &self.data,
            graphed: &graphed,
            replay,
            area,
            window,
            ignore_outliers: self.ignore_outliers,
            color_by_entity: !self.stacking.by_entity && graphed.len() > 1,
        };
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let ruler = Rect::from_min_max(rect.min, pos2(rect.right(), area.top()));
        timeaxis::ruler(ui, &painter, ruler, area, Some(area), window);

        if lay.bands.is_empty() {
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                format!("No series match \"{}\"", self.filter.trim()),
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
        }
        for (k, band) in lay.bands.iter().enumerate() {
            view.band(ui, &painter, &lay, band, k, self.filled);
        }
        let toggled = view.entity_headers(ui, &painter, &lay, pins);
        if view.color_by_entity {
            view.legend(ui, &painter, rect);
        }
        view.playhead(ui, &painter, &lay, playback.time());

        let entities = if lay.blocks.is_empty() {
            let mut used: Vec<usize> = lay.bands.iter().flat_map(|b| b.members.iter().map(|m| m.entity)).collect();
            used.sort_unstable();
            used.dedup();
            used.len()
        } else {
            lay.blocks.len()
        };
        self.shown = (entities, lay.bands.iter().map(|b| b.members.len()).sum());

        if !self.input.is_dragging()
            && let Some(p) = response.hover_pos().filter(|p| area.contains(*p))
        {
            view.hover(ui, &painter, &lay, p);
        }
        if let Some(id) = toggled {
            pins.toggle(id);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, window: &mut TimeWindow, selected: Option<i32>, pins: &mut Pins) {
        ui.horizontal_wrapped(|ui| {
            ui.toggle_value(&mut self.stacking.by_entity, "Per entity")
                .on_hover_text("Give each entity its own block. Off: compare entities in shared bands");
            ui.toggle_value(&mut self.stacking.by_parameter, "Per parameter")
                .on_hover_text("Give each parameter its own band");
            ui.toggle_value(&mut self.stacking.by_depth, "By depth")
                .on_hover_text("Band nested parameter names (a.b.c) by how deep they are");
            ui.separator();
            ui.toggle_value(&mut self.filled, "Filled");
            ui.toggle_value(&mut self.ignore_outliers, "Clip spikes")
                .on_hover_text("Scale to the typical range, clipping values far from the mean");
            ui.toggle_value(&mut window.follow, "Follow").on_hover_text("Keep the playhead in view");
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter (a|b)…").desired_width(140.0));
            if !self.filter.is_empty() && ui.small_button("✕").on_hover_text("Clear filter").clicked() {
                self.filter.clear();
            }
            ui.separator();
            if let Some(id) = selected.filter(|&id| self.data.has_series(id)) {
                let label = if pins.contains(id) { "Unpin selection" } else { "Pin selection" };
                if ui.button(label).on_hover_text("Keep this entity graphed while selecting others").clicked() {
                    pins.toggle(id);
                }
            }
            if !pins.is_empty() && ui.button("Clear pins").clicked() {
                pins.clear();
            }
            if ui
                .add_enabled(!window.is_full(), egui::Button::new("Fit"))
                .on_hover_text("Show the whole capture (double-click the graph)")
                .clicked()
            {
                window.fill();
            }
            ui.add(
                egui::Label::new(egui::RichText::new(format!("{:.2}s – {:.2}s", window.start(), window.end())).weak())
                    .wrap_mode(egui::TextWrapMode::Extend),
            );
        });
    }

    fn hint(&self, ui: &mut egui::Ui, replay: &Replay, selected: &mut Option<i32>) {
        ui.add_space(6.0);
        ui.weak(if selected.is_some() {
            "The selected entity recorded no numeric values."
        } else {
            "Select an entity to graph its numeric values."
        });
        if self.data.entities().next().is_none() {
            ui.weak("This capture has no numeric values at all.");
            return;
        }
        ui.label("Entities with numeric values:");
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for id in self.data.entities() {
                let name = replay.entity(id).map_or_else(|| format!("#{id}"), |e| e.display_name());
                let n = self.data.series(id).len();
                if ui.link(format!("{name} ({n} series)")).clicked() {
                    *selected = Some(id);
                }
            }
        });
    }
}

/// Everything painting needs for one frame.
struct View<'a> {
    data: &'a GraphData,
    graphed: &'a [i32],
    replay: &'a Replay,
    area: Rect,
    window: &'a TimeWindow,
    ignore_outliers: bool,
    /// Entities share bands, so every series is coloured by its entity, as in the legend.
    color_by_entity: bool,
}

impl View<'_> {
    fn series(&self, m: Member) -> &Series {
        &self.data.series(self.graphed[m.entity])[m.series]
    }

    fn entity_name(&self, slot: usize) -> String {
        let id = self.graphed[slot];
        self.replay.entity(id).map_or_else(|| format!("#{id}"), |e| e.display_name())
    }

    fn color(&self, m: Member) -> Color32 {
        let index = if self.color_by_entity { m.entity } else { self.data.name_index(&self.series(m).name) };
        PALETTE[index % PALETTE.len()]
    }

    fn x_of(&self, t: f64) -> f32 {
        self.area.left() + self.window.fraction_of(t) as f32 * self.area.width()
    }

    fn band_rect(&self, top: f32, bottom: f32) -> Rect {
        Rect::from_min_max(
            pos2(self.area.left(), self.area.top() + top * self.area.height()),
            pos2(self.area.right(), self.area.top() + bottom * self.area.height()),
        )
    }

    fn scale(&self, band: &Band) -> Scale {
        band.members
            .iter()
            .map(|&m| self.series(m).scale(self.ignore_outliers))
            .reduce(Scale::union)
            .unwrap_or(Scale { lo: 0.0, hi: 1.0 })
    }

    /// Maps a value to a y coordinate inside `rect`, clamped to it.
    fn y_of(rect: Rect, scale: Scale, v: f32) -> f32 {
        let inner = rect.shrink2(vec2(0.0, 2.0));
        inner.bottom() - scale.fraction(v).clamp(0.0, 1.0) * inner.height()
    }

    fn band(&self, ui: &egui::Ui, painter: &Painter, lay: &Layout, band: &Band, index: usize, filled: bool) {
        let visuals = ui.visuals();
        let rect = self.band_rect(band.top, band.bottom);
        if index % 2 == 1 {
            painter.rect_filled(rect, 0.0, visuals.faint_bg_color.gamma_multiply(0.7));
        }
        let scale = self.scale(band);
        let (t0, t1) = (self.window.start(), self.window.end());

        for &m in &band.members {
            let color = self.color(m);
            let trace = trace(self.series(m), t0, t1, self.area.width());
            if filled && !trace.fill.is_empty() {
                let mut mesh = Mesh::default();
                let fill = color.gamma_multiply(if band.members.len() == 1 { 0.22 } else { 0.08 });
                for &(x0, x1, lo, hi) in &trace.fill {
                    let (y_hi, y_lo) = (Self::y_of(rect, scale, hi), Self::y_of(rect, scale, lo));
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(self.area.left() + x0, y_hi), pos2(self.area.left() + x1, y_lo)),
                        fill,
                    );
                }
                painter.add(Shape::mesh(mesh));
            }
            let points: Vec<Pos2> =
                trace.line.iter().map(|&(x, v)| pos2(self.area.left() + x, Self::y_of(rect, scale, v))).collect();
            if points.len() >= 2 {
                painter.add(Shape::line(points, Stroke::new(1.25_f32, color)));
            }
        }

        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0_f32, visuals.weak_text_color().gamma_multiply(0.3)),
        );

        // Leave room for the entity header in the first band of each entity.
        let under_header = lay.blocks.iter().any(|b| b.top == band.top);
        let title_y = rect.top() + if under_header { HEADER_HEIGHT + 2.0 } else { 2.0 };
        let weak = visuals.weak_text_color();
        let backdrop = visuals.extreme_bg_color.gamma_multiply(0.8);
        if rect.bottom() - title_y >= 12.0 {
            tag(painter, pos2(rect.left() + 4.0, title_y), Align2::LEFT_TOP, &band.title, weak, backdrop);
        }
        if rect.height() >= 30.0 {
            painter.text(
                rect.right_top() + vec2(-4.0, 2.0),
                Align2::RIGHT_TOP,
                format_value(scale.hi),
                FontId::proportional(10.0),
                weak,
            );
            painter.text(
                rect.right_bottom() + vec2(-4.0, -2.0),
                Align2::RIGHT_BOTTOM,
                format_value(scale.lo),
                FontId::proportional(10.0),
                weak,
            );
        }
    }

    /// Entity names with a pin button. Returns an entity whose pin was toggled.
    fn entity_headers(&self, ui: &mut egui::Ui, painter: &Painter, lay: &Layout, pins: &Pins) -> Option<i32> {
        let visuals = ui.visuals().clone();
        let mut toggled = None;
        for block in &lay.blocks {
            let rect = self.band_rect(block.top, block.bottom);
            if block.top > 0.0 {
                painter.line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.5_f32, visuals.weak_text_color().gamma_multiply(0.6)),
                );
            }
            let id = self.graphed[block.entity];
            let galley = painter.layout_no_wrap(
                self.entity_name(block.entity),
                FontId::proportional(13.0),
                visuals.strong_text_color(),
            );
            let at = rect.left_top() + vec2(4.0, 2.0);
            let header = Rect::from_min_size(at, galley.size()).expand(2.0);
            painter.rect_filled(header, 3.0, visuals.extreme_bg_color.gamma_multiply(0.85));
            painter.galley(at, galley, visuals.strong_text_color());
            if rect.height() >= HEADER_HEIGHT * 2.0 {
                let is_pinned = pins.contains(id);
                let button = egui::Button::new(RichText::new(if is_pinned { "Unpin" } else { "Pin" }).small())
                    .selected(is_pinned);
                let button_rect = Rect::from_min_size(header.right_top() + vec2(4.0, 0.0), vec2(42.0, header.height()));
                if ui.put(button_rect, button).on_hover_text("Keep graphing this entity whatever is selected").clicked()
                {
                    toggled = Some(id);
                }
            }
        }
        toggled
    }

    /// Which colour is which entity, when entities share bands.
    fn legend(&self, ui: &egui::Ui, painter: &Painter, rect: Rect) {
        let names: Vec<_> = (0..self.graphed.len())
            .map(|slot| {
                painter.layout_no_wrap(self.entity_name(slot), FontId::proportional(11.0), ui.visuals().text_color())
            })
            .collect();
        let width: f32 = names.iter().map(|g| g.size().x + 22.0).sum::<f32>() + 6.0;
        let backdrop = Rect::from_min_max(
            pos2(rect.right() - width - 4.0, rect.top()),
            pos2(rect.right(), rect.top() + RULER_HEIGHT - 1.0),
        );
        painter.rect_filled(backdrop, 0.0, ui.visuals().faint_bg_color);
        let mut x = rect.right() - 6.0;
        for slot in (0..self.graphed.len()).rev() {
            let color = PALETTE[slot % PALETTE.len()];
            let galley = names[slot].clone();
            x -= galley.size().x;
            let y = rect.top() + (RULER_HEIGHT - galley.size().y) * 0.5;
            painter.galley(pos2(x, y), galley, color);
            x -= 12.0;
            painter.rect_filled(
                Rect::from_center_size(pos2(x + 4.0, rect.top() + RULER_HEIGHT * 0.5), vec2(8.0, 8.0)),
                1.0,
                color,
            );
            x -= 10.0;
        }
    }

    fn playhead(&self, ui: &egui::Ui, painter: &Painter, lay: &Layout, t: f64) {
        if t < self.window.start() || t > self.window.end() {
            return;
        }
        let x = self.x_of(t);
        timeaxis::playhead(painter, self.window, self.area, t, self.area.top() - RULER_HEIGHT, self.area.bottom());
        let backdrop = ui.visuals().extreme_bg_color.gamma_multiply(0.8);
        for band in &lay.bands {
            let rect = self.band_rect(band.top, band.bottom);
            if rect.height() < 16.0 || band.members.len() > MAX_PLAYHEAD_LABELS {
                continue;
            }
            let scale = self.scale(band);
            for &m in &band.members {
                let s = self.series(m);
                let Some(v) = s.value_at(t) else {
                    continue;
                };
                let text = if self.color_by_entity {
                    format!("{}: {}", self.entity_name(m.entity), format_value(v))
                } else if band.members.len() > 1 {
                    format!("{} {}", s.name, format_value(v))
                } else {
                    format_value(v)
                };
                let y = Self::y_of(rect, scale, v).clamp(rect.top() + 7.0, rect.bottom() - 7.0);
                let color = self.color(m);
                let width = painter.layout_no_wrap(text.clone(), FontId::proportional(11.0), color).size().x;
                // The entity header sits in the top-left corner of an entity's first band; a label
                // that would land on it drops below it instead.
                let under_header = lay.blocks.iter().any(|b| b.top == band.top)
                    && y < rect.top() + HEADER_HEIGHT + 4.0
                    && x < self.area.left() + 260.0;
                let y = if under_header { (rect.top() + HEADER_HEIGHT + 9.0).min(rect.bottom() - 7.0) } else { y };
                // Right of the line, unless that would run off the graph.
                let (at, align) = if x + 4.0 + width <= self.area.right() {
                    (pos2(x + 4.0, y), Align2::LEFT_CENTER)
                } else {
                    (pos2(x - 4.0, y), Align2::RIGHT_CENTER)
                };
                tag(painter, at, align, &text, color, backdrop);
            }
        }
    }

    /// A guide line at the pointer and a read-out of the values there for the band under it.
    fn hover(&self, ui: &egui::Ui, painter: &Painter, lay: &Layout, p: Pos2) {
        let t = self.window.time_at(f64::from((p.x - self.area.left()) / self.area.width().max(1.0)));
        crate::widgets::guide(
            painter,
            p.x,
            self.area.top(),
            self.area.bottom(),
            Color32::from_gray(128).gamma_multiply(0.6),
        );
        let Some(band) = lay.bands.iter().find(|b| {
            let r = self.band_rect(b.top, b.bottom);
            p.y >= r.top() && p.y <= r.bottom()
        }) else {
            return;
        };

        let frame = self.replay.frame_for_time(t);
        let mut lines = vec![(format!("{t:.3}s  ·  frame {frame}"), ui.visuals().strong_text_color())];
        let mut members: Vec<(Member, Option<f32>)> =
            band.members.iter().map(|&m| (m, self.series(m).value_at(t))).collect();
        // Busy bands list the largest values first.
        if members.len() > 1 {
            members.sort_by(|a, b| b.1.unwrap_or(f32::MIN).total_cmp(&a.1.unwrap_or(f32::MIN)));
        }
        for (m, value) in members {
            let s = self.series(m);
            let who = if self.color_by_entity {
                format!("{} · {}", self.entity_name(m.entity), s.name)
            } else {
                s.name.clone()
            };
            let value = value.map_or_else(|| "–".to_string(), format_value);
            lines.push((format!("{who}: {value}"), self.color(m)));
        }
        // The panel may reach up over the ruler when the graph is short.
        let bounds = Rect::from_min_max(pos2(self.area.left(), self.area.top() - RULER_HEIGHT), self.area.max);
        readout(ui, painter, bounds, p, lines);
    }
}
