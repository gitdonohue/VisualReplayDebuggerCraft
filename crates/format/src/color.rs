/// A named colour, stored in captures as its index in the table below.
///
/// The order matches the `System.Windows.Media.Colors` enum used by the original serializers and
/// must never change, since capture files refer to colours by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Color(pub u32);

/// (name, 0xAARRGGBB)
const TABLE: [(&str, u32); 141] = [
    ("AliceBlue", 0xFFF0F8FF),
    ("PaleGoldenrod", 0xFFEEE8AA),
    ("Orchid", 0xFFDA70D6),
    ("OrangeRed", 0xFFFF4500),
    ("Orange", 0xFFFFA500),
    ("OliveDrab", 0xFF6B8E23),
    ("Olive", 0xFF808000),
    ("OldLace", 0xFFFDF5E6),
    ("Navy", 0xFF000080),
    ("NavajoWhite", 0xFFFFDEAD),
    ("Moccasin", 0xFFFFE4B5),
    ("MistyRose", 0xFFFFE4E1),
    ("MintCream", 0xFFF5FFFA),
    ("MidnightBlue", 0xFF191970),
    ("MediumVioletRed", 0xFFC71585),
    ("MediumTurquoise", 0xFF48D1CC),
    ("MediumSpringGreen", 0xFF00FA9A),
    ("MediumSlateBlue", 0xFF7B68EE),
    ("LightSkyBlue", 0xFF87CEFA),
    ("LightSlateGray", 0xFF778899),
    ("LightSteelBlue", 0xFFB0C4DE),
    ("LightYellow", 0xFFFFFFE0),
    ("Lime", 0xFF00FF00),
    ("LimeGreen", 0xFF32CD32),
    ("PaleGreen", 0xFF98FB98),
    ("Linen", 0xFFFAF0E6),
    ("Maroon", 0xFF800000),
    ("MediumAquamarine", 0xFF66CDAA),
    ("MediumBlue", 0xFF0000CD),
    ("MediumOrchid", 0xFFBA55D3),
    ("MediumPurple", 0xFF9370DB),
    ("MediumSeaGreen", 0xFF3CB371),
    ("Magenta", 0xFFFF00FF),
    ("PaleTurquoise", 0xFFAFEEEE),
    ("PaleVioletRed", 0xFFDB7093),
    ("PapayaWhip", 0xFFFFEFD5),
    ("SlateGray", 0xFF708090),
    ("Snow", 0xFFFFFAFA),
    ("SpringGreen", 0xFF00FF7F),
    ("SteelBlue", 0xFF4682B4),
    ("Tan", 0xFFD2B48C),
    ("Teal", 0xFF008080),
    ("SlateBlue", 0xFF6A5ACD),
    ("Thistle", 0xFFD8BFD8),
    ("Transparent", 0x00FFFFFF),
    ("Turquoise", 0xFF40E0D0),
    ("Violet", 0xFFEE82EE),
    ("Wheat", 0xFFF5DEB3),
    ("White", 0xFFFFFFFF),
    ("WhiteSmoke", 0xFFF5F5F5),
    ("Tomato", 0xFFFF6347),
    ("LightSeaGreen", 0xFF20B2AA),
    ("SkyBlue", 0xFF87CEEB),
    ("Sienna", 0xFFA0522D),
    ("PeachPuff", 0xFFFFDAB9),
    ("Peru", 0xFFCD853F),
    ("Pink", 0xFFFFC0CB),
    ("Plum", 0xFFDDA0DD),
    ("PowderBlue", 0xFFB0E0E6),
    ("Purple", 0xFF800080),
    ("Silver", 0xFFC0C0C0),
    ("Red", 0xFFFF0000),
    ("RoyalBlue", 0xFF4169E1),
    ("SaddleBrown", 0xFF8B4513),
    ("Salmon", 0xFFFA8072),
    ("SandyBrown", 0xFFF4A460),
    ("SeaGreen", 0xFF2E8B57),
    ("SeaShell", 0xFFFFF5EE),
    ("RosyBrown", 0xFFBC8F8F),
    ("Yellow", 0xFFFFFF00),
    ("LightSalmon", 0xFFFFA07A),
    ("LightGreen", 0xFF90EE90),
    ("DarkRed", 0xFF8B0000),
    ("DarkOrchid", 0xFF9932CC),
    ("DarkOrange", 0xFFFF8C00),
    ("DarkOliveGreen", 0xFF556B2F),
    ("DarkMagenta", 0xFF8B008B),
    ("DarkKhaki", 0xFFBDB76B),
    ("DarkGreen", 0xFF006400),
    ("DarkGray", 0xFFA9A9A9),
    ("DarkGoldenrod", 0xFFB8860B),
    ("DarkCyan", 0xFF008B8B),
    ("DarkBlue", 0xFF00008B),
    ("Cyan", 0xFF00FFFF),
    ("Crimson", 0xFFDC143C),
    ("Cornsilk", 0xFFFFF8DC),
    ("CornflowerBlue", 0xFF6495ED),
    ("Coral", 0xFFFF7F50),
    ("Chocolate", 0xFFD2691E),
    ("AntiqueWhite", 0xFFFAEBD7),
    ("Aqua", 0xFF00FFFF),
    ("Aquamarine", 0xFF7FFFD4),
    ("Azure", 0xFFF0FFFF),
    ("Beige", 0xFFF5F5DC),
    ("Bisque", 0xFFFFE4C4),
    ("DarkSalmon", 0xFFE9967A),
    ("Black", 0xFF000000),
    ("Blue", 0xFF0000FF),
    ("BlueViolet", 0xFF8A2BE2),
    ("Brown", 0xFFA52A2A),
    ("BurlyWood", 0xFFDEB887),
    ("CadetBlue", 0xFF5F9EA0),
    ("Chartreuse", 0xFF7FFF00),
    ("BlanchedAlmond", 0xFFFFEBCD),
    ("DarkSeaGreen", 0xFF8FBC8F),
    ("DarkSlateBlue", 0xFF483D8B),
    ("DarkSlateGray", 0xFF2F4F4F),
    ("HotPink", 0xFFFF69B4),
    ("IndianRed", 0xFFCD5C5C),
    ("Indigo", 0xFF4B0082),
    ("Ivory", 0xFFFFFFF0),
    ("Khaki", 0xFFF0E68C),
    ("Lavender", 0xFFE6E6FA),
    ("Honeydew", 0xFFF0FFF0),
    ("LavenderBlush", 0xFFFFF0F5),
    ("LemonChiffon", 0xFFFFFACD),
    ("LightBlue", 0xFFADD8E6),
    ("LightCoral", 0xFFF08080),
    ("LightCyan", 0xFFE0FFFF),
    ("LightGoldenrodYellow", 0xFFFAFAD2),
    ("LightGray", 0xFFD3D3D3),
    ("LawnGreen", 0xFF7CFC00),
    ("LightPink", 0xFFFFB6C1),
    ("GreenYellow", 0xFFADFF2F),
    ("Gray", 0xFF808080),
    ("DarkTurquoise", 0xFF00CED1),
    ("DarkViolet", 0xFF9400D3),
    ("DeepPink", 0xFFFF1493),
    ("DeepSkyBlue", 0xFF00BFFF),
    ("DimGray", 0xFF696969),
    ("DodgerBlue", 0xFF1E90FF),
    ("Green", 0xFF008000),
    ("Firebrick", 0xFFB22222),
    ("ForestGreen", 0xFF228B22),
    ("Fuchsia", 0xFFFF00FF),
    ("Gainsboro", 0xFFDCDCDC),
    ("GhostWhite", 0xFFF8F8FF),
    ("Gold", 0xFFFFD700),
    ("Goldenrod", 0xFFDAA520),
    ("FloralWhite", 0xFFFFFAF0),
    ("YellowGreen", 0xFF9ACD32),
];

impl Color {
    pub const BLACK: Color = Color(96);
    pub const RED: Color = Color(61);
    pub const GREEN: Color = Color(131);
    pub const BLUE: Color = Color(97);
    pub const WHITE: Color = Color(48);
    pub const YELLOW: Color = Color(69);

    /// Number of named colours.
    pub const COUNT: u32 = TABLE.len() as u32;

    /// The colour's name, or `None` for an index outside the table.
    pub fn name(self) -> Option<&'static str> {
        TABLE.get(self.0 as usize).map(|e| e.0)
    }

    /// Straight (non-premultiplied) RGBA. Unknown indices render as opaque magenta.
    pub fn rgba(self) -> [u8; 4] {
        let argb = TABLE.get(self.0 as usize).map_or(0xFFFF00FF, |e| e.1);
        [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8]
    }

    /// Looks a colour up by case-insensitive name.
    pub fn from_name(name: &str) -> Option<Color> {
        TABLE.iter().position(|e| e.0.eq_ignore_ascii_case(name)).map(|i| Color(i as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_match_the_windows_enum() {
        assert_eq!(Color(0).name(), Some("AliceBlue"));
        assert_eq!(Color(44).name(), Some("Transparent"));
        assert_eq!(Color::RED.name(), Some("Red"));
        assert_eq!(Color::BLACK.name(), Some("Black"));
        assert_eq!(Color::BLUE.name(), Some("Blue"));
        assert_eq!(Color::GREEN.name(), Some("Green"));
        assert_eq!(Color::WHITE.name(), Some("White"));
        assert_eq!(Color::YELLOW.name(), Some("Yellow"));
        assert_eq!(Color(140).name(), Some("YellowGreen"));
        assert_eq!(Color(141).name(), None);
    }

    #[test]
    fn rgba_and_lookup() {
        assert_eq!(Color::RED.rgba(), [255, 0, 0, 255]);
        assert_eq!(Color(44).rgba()[3], 0);
        assert_eq!(Color::from_name("dodgerblue"), Some(Color(130)));
        assert_eq!(Color(9999).rgba(), [255, 0, 255, 255]);
    }
}
