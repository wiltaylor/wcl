//! Text-metrics helpers shared by the label renderer and the
//! effective-shape-dimensions calculation.
//!
//! Everything here is heuristic: a real text-measurement step would
//! need a font / glyph table at build time. The constants below
//! approximate a generic sans-serif at typical body weight.

/// Default font-size used when nothing else constrains it.
pub(crate) const DEFAULT_FONT_SIZE: f64 = 14.0;
/// Average glyph width as a fraction of font size, for sans-serif.
pub(crate) const CHAR_RATIO: f64 = 0.55;
/// Line height as a multiple of font size.
pub(crate) const LINE_HEIGHT: f64 = 1.2;
/// Horizontal padding around text inside a shape.
pub(crate) const H_PAD: f64 = 16.0;
/// Vertical padding around text inside a shape.
pub(crate) const V_PAD: f64 = 12.0;
/// Don't let auto-fit shrink below this for legibility.
pub(crate) const MIN_FONT_SIZE: f64 = 8.0;

pub(crate) struct TextMetrics {
    pub lines: Vec<String>,
    pub max_chars: usize,
}

/// Split text on `\n`. An empty input still yields one (empty) line
/// so callers don't have to special-case zero.
pub(crate) fn measure(text: &str) -> TextMetrics {
    let lines: Vec<String> = if text.is_empty() {
        vec![String::new()]
    } else {
        text.split('\n').map(|s| s.to_string()).collect()
    };
    let max_chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    TextMetrics { lines, max_chars }
}

/// Smallest shape that comfortably holds `text` at
/// `DEFAULT_FONT_SIZE`. Returned dims include `H_PAD` / `V_PAD`.
pub(crate) fn min_shape_dims(text: &str) -> (f64, f64) {
    let m = measure(text);
    let needed_w = (m.max_chars as f64) * DEFAULT_FONT_SIZE * CHAR_RATIO + H_PAD;
    let needed_h = (m.lines.len() as f64) * DEFAULT_FONT_SIZE * LINE_HEIGHT + V_PAD;
    (needed_w, needed_h)
}

/// Largest font size that fits `text` inside the given inner box
/// (the box should already have padding subtracted). Capped at
/// `DEFAULT_FONT_SIZE` so we never grow above the design baseline,
/// and floored at `MIN_FONT_SIZE` so we don't render unreadable
/// pixel soup.
pub(crate) fn fit_font_size(text: &str, inner_w: f64, inner_h: f64) -> f64 {
    let m = measure(text);
    if m.max_chars == 0 || m.lines.is_empty() {
        return DEFAULT_FONT_SIZE;
    }
    let by_w = (inner_w / (m.max_chars as f64 * CHAR_RATIO)).max(0.0);
    let by_h = (inner_h / (m.lines.len() as f64 * LINE_HEIGHT)).max(0.0);
    DEFAULT_FONT_SIZE.min(by_w).min(by_h).max(MIN_FONT_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_splits_on_newlines() {
        let m = measure("hello\nworld");
        assert_eq!(m.lines, vec!["hello", "world"]);
        assert_eq!(m.max_chars, 5);
    }

    #[test]
    fn min_dims_account_for_lines_and_chars() {
        let (w, h) = min_shape_dims("ab\ncd\nef");
        // 3 lines × 14 × 1.2 + 12 = 62.4
        assert!((h - (3.0 * 14.0 * 1.2 + 12.0)).abs() < 1e-6);
        // 2 chars × 14 × 0.55 + 16 = 31.4
        assert!((w - (2.0 * 14.0 * 0.55 + 16.0)).abs() < 1e-6);
    }

    #[test]
    fn fit_font_size_shrinks_for_narrow_box() {
        // 10 chars at default font would want ~77 wide; inner_w=44
        // (60-16 pad) forces a smaller size.
        let f = fit_font_size("abcdefghij", 44.0, 100.0);
        assert!(f < DEFAULT_FONT_SIZE);
        assert!(f >= MIN_FONT_SIZE);
    }

    #[test]
    fn fit_font_size_caps_at_default() {
        let f = fit_font_size("ok", 1000.0, 1000.0);
        assert_eq!(f, DEFAULT_FONT_SIZE);
    }
}
