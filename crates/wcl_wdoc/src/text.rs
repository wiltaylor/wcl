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

/// A measured block of text: its wrapped lines and the widest one.
pub(crate) struct TextMetrics {
    /// The text after wrapping.
    pub lines: Vec<String>,
    /// Character count of the longest line.
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

/// Greedily word-wrap `text` so each line's estimated width at `font_size`
/// fits `max_w` px. Existing newlines stay as hard breaks; a single word wider
/// than `max_w` keeps its own line (no mid-word splitting). Used to keep an
/// auto-fit label (e.g. a circle `node`'s centred text) inside its box instead
/// of shrinking one long line to pixel soup or overflowing.
pub(crate) fn wrap(text: &str, max_w: f64, font_size: f64) -> String {
    if max_w <= 0.0 || font_size <= 0.0 {
        return text.to_string();
    }
    let max_chars = (max_w / (font_size * CHAR_RATIO)).floor().max(1.0) as usize;
    let mut lines = Vec::new();
    for hard in text.split('\n') {
        let mut line = String::new();
        for word in hard.split_whitespace() {
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + word.chars().count() <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Word-wrap `text` to an inner box (padding already subtracted) and pick the
/// font size to render it at. Two passes: wrap at the default size, shrink to
/// fit the box, then re-wrap at that smaller size so each line is as full as
/// the final size allows (avoids a long label collapsing into a stack of
/// one-word lines). Returns the wrapped text and its font size.
pub(crate) fn wrap_to_box(text: &str, inner_w: f64, inner_h: f64) -> (String, f64) {
    if inner_w <= 0.0 {
        return (text.to_string(), fit_font_size(text, inner_w, inner_h));
    }
    let first = wrap(text, inner_w, DEFAULT_FONT_SIZE);
    let fs = fit_font_size(&first, inner_w, inner_h);
    let wrapped = wrap(text, inner_w, fs);
    let font = fit_font_size(&wrapped, inner_w, inner_h);
    (wrapped, font)
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

    #[test]
    fn wrap_breaks_on_word_boundaries_to_fit_width() {
        // ~70px at default font ≈ 9 chars/line, so a long name wraps.
        let w = wrap(
            "Separation of Data and Presentation",
            70.0,
            DEFAULT_FONT_SIZE,
        );
        assert!(w.contains('\n'), "expected wrapping, got {w:?}");
        // Every wrapped line that isn't a single long word fits the budget.
        let max_chars = (70.0 / (DEFAULT_FONT_SIZE * CHAR_RATIO)).floor() as usize;
        for line in w.split('\n') {
            assert!(
                line.split_whitespace().count() <= 1 || line.chars().count() <= max_chars,
                "line {line:?} exceeds {max_chars} chars"
            );
        }
        // No words are lost.
        assert_eq!(w.split_whitespace().count(), 5);
    }

    #[test]
    fn wrap_keeps_hard_newlines() {
        assert_eq!(wrap("a\nb", 1000.0, DEFAULT_FONT_SIZE), "a\nb");
    }
}
