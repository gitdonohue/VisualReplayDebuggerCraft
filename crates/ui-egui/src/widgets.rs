//! Small drawing helpers shared by the panels.

use std::collections::BTreeSet;

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, pos2, vec2};

/// Small text on a backdrop, so it stays legible over lines and fills.
pub fn tag(painter: &Painter, at: Pos2, align: Align2, text: &str, color: Color32, backdrop: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let rect = align.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand(1.0), 2.0, backdrop);
    painter.galley(rect.min, galley, color);
    rect
}

/// A panel of text lines beside the pointer, kept inside `bounds`. The first line is a heading.
/// If the lines do not all fit, the last ones are replaced by "and N more".
pub fn readout(ui: &Ui, painter: &Painter, bounds: Rect, pointer: Pos2, mut lines: Vec<(String, Color32)>) {
    let visuals = ui.visuals();
    let font = FontId::proportional(12.0);
    let row_height = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
    let fit = (((bounds.height() - 8.0) / row_height) as usize).max(2);
    if lines.len() > fit {
        let hidden = lines.len() - (fit - 1);
        lines.truncate(fit - 1);
        lines.push((format!("and {hidden} more"), visuals.weak_text_color()));
    }

    let galleys: Vec<_> =
        lines.into_iter().map(|(text, color)| painter.layout_no_wrap(text, font.clone(), color)).collect();
    let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 12.0;
    let height = galleys.iter().map(|g| g.size().y + 2.0).sum::<f32>() + 8.0;
    // Beside the pointer, flipped to the other side near the right edge, kept inside the bounds.
    let x = if pointer.x + 14.0 + width <= bounds.right() { pointer.x + 14.0 } else { pointer.x - 14.0 - width };
    let y = (pointer.y - height * 0.5).clamp(bounds.top(), (bounds.bottom() - height).max(bounds.top()));
    let panel = Rect::from_min_size(pos2(x.max(bounds.left()), y), vec2(width, height));
    painter.rect(panel, 4.0, visuals.extreme_bg_color.gamma_multiply(0.95), visuals.window_stroke, StrokeKind::Inside);
    let mut row_y = panel.top() + 4.0;
    for galley in galleys {
        let h = galley.size().y;
        painter.galley(pos2(panel.left() + 6.0, row_y), galley, Color32::PLACEHOLDER);
        row_y += h + 2.0;
    }
}

/// A thin line, for guides.
pub fn guide(painter: &Painter, x: f32, from: f32, to: f32, color: Color32) {
    painter.line_segment([pos2(x, from), pos2(x, to)], Stroke::new(1.0_f32, color));
}

/// A checkbox list of named items that can be hidden, with All/None buttons.
pub fn category_menu<'a>(
    ui: &mut Ui,
    id: &str,
    items: impl Iterator<Item = (&'a str, &'a String)> + Clone,
    hidden: &mut BTreeSet<String>,
    empty_message: &str,
) {
    ui.horizontal(|ui| {
        if ui.small_button("All").clicked() {
            hidden.clear();
        }
        if ui.small_button("None").clicked() {
            hidden.extend(items.clone().map(|(_, k)| k.clone()));
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt(id).max_height(320.0).show(ui, |ui| {
        let mut any = false;
        for (label, key) in items {
            any = true;
            let mut shown = !hidden.contains(key);
            let text = if label.is_empty() { "(none)" } else { label };
            if ui.checkbox(&mut shown, text).changed() {
                if shown {
                    hidden.remove(key);
                } else {
                    hidden.insert(key.clone());
                }
            }
        }
        if !any {
            ui.weak(empty_message);
        }
    });
}

/// A colour for a numbered label: the golden angle keeps neighbouring numbers far apart in hue.
/// `fill` is for backgrounds, `ink` for text and outlines on top of it.
pub fn label_colors(index: usize, dark_mode: bool) -> (Color32, Color32) {
    let hue = (index as f32 * 0.618_034).fract();
    let (fill, ink) = if dark_mode {
        (hsl(hue, 0.45, 0.30), hsl(hue, 0.70, 0.82))
    } else {
        (hsl(hue, 0.55, 0.82), hsl(hue, 0.65, 0.22))
    };
    (fill, ink)
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_labels_get_different_colours() {
        for dark in [false, true] {
            let colors: Vec<_> = (0..12).map(|i| label_colors(i, dark).0).collect();
            for pair in colors.windows(2) {
                assert_ne!(pair[0], pair[1]);
            }
            let mut unique = colors.clone();
            unique.sort_by_key(|c| c.to_array());
            unique.dedup();
            assert_eq!(unique.len(), colors.len());
        }
    }

    #[test]
    fn ink_contrasts_with_fill() {
        let luma = |c: Color32| 0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b());
        for i in 0..20 {
            let (fill, ink) = label_colors(i, false);
            assert!(luma(fill) - luma(ink) > 90.0, "light: {i}");
            let (fill, ink) = label_colors(i, true);
            assert!(luma(ink) - luma(fill) > 90.0, "dark: {i}");
        }
    }

    #[test]
    fn hsl_primaries() {
        assert_eq!(hsl(0.0, 1.0, 0.5), Color32::from_rgb(255, 0, 0));
        assert_eq!(hsl(1.0 / 3.0, 1.0, 0.5), Color32::from_rgb(0, 255, 0));
        assert_eq!(hsl(2.0 / 3.0, 1.0, 0.5), Color32::from_rgb(0, 0, 255));
        assert_eq!(hsl(0.3, 0.0, 0.5), Color32::from_rgb(128, 128, 128));
    }
}
