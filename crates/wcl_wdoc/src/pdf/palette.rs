//! Colour palette for the PDF backend.
//!
//! The PDF has no CSS, so embedded SVG can't pick up the document theme the way
//! the web output does. [`Palette`] supplies the two things the SVG embed pass
//! needs: the concrete foreground colour that `currentColor` resolves to, and a
//! small stylesheet giving the chart series classes a fill (they emit no inline
//! fill on purpose). Full theme-block resolution arrives with book assembly;
//! for now this is a fixed, print-friendly dark-on-white palette.

/// A resolved colour palette.
pub(crate) struct Palette {
    /// Foreground (text / stroke) colour, as `#rrggbb`.
    fg: &'static str,
    /// The eight chart-series fills, as `#rrggbb`.
    series: [&'static str; 8],
}

impl Default for Palette {
    fn default() -> Self {
        // The Forge light chart palette (lib/theme.wcl order) on its body
        // foreground — legible on the white PDF page and consistent with the
        // web default theme.
        Self {
            fg: "#3d4654",
            series: [
                "#0069ca", "#00792f", "#c17000", "#c90019", "#006fa3", "#004573", "#7a2c00",
                "#9e0000",
            ],
        }
    }
}

impl Palette {
    /// The foreground colour `currentColor` resolves to in embedded SVG.
    pub(crate) fn fg_hex(&self) -> String {
        self.fg.to_string()
    }

    /// A stylesheet handed to usvg supplying the chart styling the browser
    /// would otherwise apply via CSS: series fills + strokes (so both bars and
    /// line strokes are coloured), and the axis / grid / label / legend rules
    /// (which the web build paints with `currentColor`, here resolved to the
    /// concrete foreground). Mirrors `lib/css-classes.wcl`.
    pub(crate) fn svg_style_sheet(&self) -> String {
        let fg = self.fg;
        let mut css = String::new();
        for (i, color) in self.series.iter().enumerate() {
            css.push_str(&format!(
                ".wdoc-series-{} {{ fill: {color}; stroke: {color}; }}",
                i + 1
            ));
        }
        css.push_str(&format!(".wdoc-axis {{ stroke: {fg}; opacity: 0.45; }}"));
        css.push_str(&format!(".wdoc-grid {{ stroke: {fg}; opacity: 0.12; }}"));
        css.push_str(&format!(
            ".wdoc-axis-label {{ fill: {fg}; opacity: 0.75; }}"
        ));
        css.push_str(&format!(
            ".wdoc-chart-title {{ fill: {fg}; font-weight: bold; }}"
        ));
        css.push_str(&format!(".wdoc-legend {{ fill: {fg}; opacity: 0.85; }}"));
        css.push_str(&format!(
            ".wdoc-point-label {{ fill: {fg}; opacity: 0.7; }}"
        ));
        css
    }
}
