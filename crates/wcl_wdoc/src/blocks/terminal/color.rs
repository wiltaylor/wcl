//! Terminal colour model: the [`Color`] enum, the resolved [`Palette`]
//! (16-colour ANSI + default fg/bg), and the hex/colour parsing helpers.

#[derive(Clone, Copy, PartialEq, Eq)]
/// A colour as a terminal names it.
pub(super) enum Color {
    /// Use the palette's default fg/bg.
    Default,
    /// 0..=255 indexed colour (0..16 themeable, then cube + greyscale).
    Indexed(u8),
    /// A direct 24-bit colour.
    Rgb(u8, u8, u8),
}

/// A resolved 16-colour palette plus default fg/bg.
pub(super) struct Palette {
    /// Default foreground.
    pub(super) fg: (u8, u8, u8),
    /// Default background.
    pub(super) bg: (u8, u8, u8),
    /// The 16 themeable ANSI colours, in index order.
    ansi: [(u8, u8, u8); 16],
}

/// The classic Tango 16-colour set — widely assumed by recordings and
/// legible on either a dark or a light background.
const TANGO: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00),
    (0xcc, 0x00, 0x00),
    (0x4e, 0x9a, 0x06),
    (0xc4, 0xa0, 0x00),
    (0x34, 0x65, 0xa4),
    (0x75, 0x50, 0x7b),
    (0x06, 0x98, 0x9a),
    (0xd3, 0xd7, 0xcf),
    (0x55, 0x57, 0x53),
    (0xef, 0x29, 0x29),
    (0x8a, 0xe2, 0x34),
    (0xfc, 0xe9, 0x4f),
    (0x72, 0x9f, 0xcf),
    (0xad, 0x7f, 0xa8),
    (0x34, 0xe2, 0xe2),
    (0xee, 0xee, 0xec),
];

impl Palette {
    /// `light` is the `(fg, bg)` the `:light` preset should use — the site
    /// theme's light-mode `fg`/`bg` when the doc is themed, else `None` for
    /// the concrete white/dark default.
    pub(super) fn new(
        preset: Option<&str>,
        fg: Option<&str>,
        bg: Option<&str>,
        light: Option<(&str, &str)>,
    ) -> Self {
        let (mut dfg, mut dbg) = match preset {
            Some("light") => {
                let (lfg, lbg) = light.unwrap_or(("#1c1c1c", "#ffffff"));
                (
                    parse_hex(lfg).unwrap_or((0x1c, 0x1c, 0x1c)),
                    parse_hex(lbg).unwrap_or((0xff, 0xff, 0xff)),
                )
            }
            _ => ((0xd0, 0xd0, 0xd0), (0x1c, 0x1c, 0x1c)),
        };
        if let Some(c) = fg.and_then(parse_hex) {
            dfg = c;
        }
        if let Some(c) = bg.and_then(parse_hex) {
            dbg = c;
        }
        Palette {
            fg: dfg,
            bg: dbg,
            ansi: TANGO,
        }
    }

    /// Resolve an indexed colour to RGB across the full 256-colour space.
    fn indexed(&self, i: u8) -> (u8, u8, u8) {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let steps = [0u8, 95, 135, 175, 215, 255];
                (
                    steps[(i / 36) as usize],
                    steps[((i / 6) % 6) as usize],
                    steps[(i % 6) as usize],
                )
            }
            _ => {
                let v = 8u16 + 10 * (i as u16 - 232);
                let v = v.min(255) as u8;
                (v, v, v)
            }
        }
    }

    /// Resolve a colour to RGB. `None` for [`Color::Default`], which the
    /// emitter renders as `currentColor` so CSS can theme it.
    pub(super) fn rgb_of(&self, c: Color) -> Option<(u8, u8, u8)> {
        match c {
            Color::Default => None,
            Color::Indexed(i) => Some(self.indexed(i)),
            Color::Rgb(r, g, b) => Some((r, g, b)),
        }
    }
}

/// Format an RGB triple as `#rrggbb`.
pub(super) fn hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// Render a foreground colour for SVG `fill`: a concrete colour as hex,
/// or the terminal's default as `currentColor` so a WCL `class`'s
/// `color` (and its dark/light modes) themes it.
pub(super) fn ink(c: Option<(u8, u8, u8)>) -> String {
    match c {
        Some(c) => hex(c),
        None => "currentColor".to_string(),
    }
}

/// Blend two colours, `t` running 0 (all `a`) to 1 (all `b`).
pub(super) fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Parse a `#rgb` / `#rrggbb` hex colour.
pub(super) fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..=i], 16).ok().map(|v| v * 17);
            Some((d(0)?, d(1)?, d(2)?))
        }
        6 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
            Some((d(0)?, d(2)?, d(4)?))
        }
        _ => None,
    }
}

/// Parse a colour field: `#rrggbb`, a 0..=255 index, or an ANSI name
/// (`red`, `bright_red`, `brightblue`, …). Unknown ⇒ `Default`.
pub(super) fn parse_color(s: &str) -> Color {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("default") {
        return Color::Default;
    }
    if let Some((r, g, b)) = parse_hex(s) {
        return Color::Rgb(r, g, b);
    }
    if let Ok(i) = s.parse::<u8>() {
        return Color::Indexed(i);
    }
    let key = s.to_ascii_lowercase();
    let key = key.replace([' ', '-'], "_");
    let (bright, base) = match key
        .strip_prefix("bright_")
        .or_else(|| key.strip_prefix("bright"))
    {
        Some(rest) => (true, rest),
        None => (false, key.as_str()),
    };
    let idx = match base {
        "black" => 0,
        "red" => 1,
        "green" => 2,
        "yellow" => 3,
        "blue" => 4,
        "magenta" | "purple" => 5,
        "cyan" => 6,
        "white" | "grey" | "gray" => 7,
        _ => return Color::Default,
    };
    Color::Indexed(if bright { idx + 8 } else { idx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_forms() {
        assert_eq!(parse_hex("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_hex("#0f0"), Some((0, 255, 0)));
        assert_eq!(parse_hex("nope"), None);
    }

    #[test]
    fn parse_color_names_and_indices() {
        assert!(matches!(parse_color("red"), Color::Indexed(1)));
        assert!(matches!(parse_color("bright_red"), Color::Indexed(9)));
        assert!(matches!(parse_color("brightblue"), Color::Indexed(12)));
        assert!(matches!(parse_color("200"), Color::Indexed(200)));
        assert!(matches!(parse_color("#102030"), Color::Rgb(16, 32, 48)));
        assert!(matches!(parse_color("default"), Color::Default));
    }
}
