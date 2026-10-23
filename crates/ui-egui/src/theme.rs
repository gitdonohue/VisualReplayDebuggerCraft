use egui::Color32;
use vrd_model::Color;

/// A capture colour adjusted to stay legible as text on the current background.
///
/// Captures pick colours without knowing the UI theme, so black or navy would vanish on a dark
/// background. Colours too close to the background are blended towards the foreground colour.
pub fn text_color(c: Color, dark_mode: bool) -> Color32 {
    let [r, g, b, a] = c.rgba();
    if a == 0 {
        return Color32::TRANSPARENT;
    }
    let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    let (target, bad) = if dark_mode { (255.0, luma < 40.0) } else { (0.0, luma > 150.0) };
    if !bad {
        return Color32::from_rgb(r, g, b);
    }
    let mix = |v: u8| (f32::from(v) * 0.45 + target * 0.55) as u8;
    Color32::from_rgb(mix(r), mix(g), mix(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_colors_are_lifted_on_dark_backgrounds() {
        let lifted = text_color(Color::BLACK, true);
        assert!(lifted.r() > 120, "{lifted:?}");
        let light_on_light = text_color(Color::WHITE, false);
        assert!(light_on_light.r() < 150, "{light_on_light:?}");
    }

    #[test]
    fn readable_colors_are_untouched() {
        assert_eq!(text_color(Color::RED, true), Color32::from_rgb(255, 0, 0));
        assert_eq!(text_color(Color::BLUE, false), Color32::from_rgb(0, 0, 255));
    }

    #[test]
    fn transparent_stays_transparent() {
        assert_eq!(text_color(Color(44), true), Color32::TRANSPARENT);
    }
}
