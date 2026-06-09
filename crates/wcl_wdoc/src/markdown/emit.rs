//! Walk a page's blocks into Markdown, the Markdown twin of
//! [`pdf::collect`](crate::pdf). It reuses the shared lowering seam
//! ([`lower_to_values`](crate::render::lower_to_values), which runs a
//! block's WCL `lower` and returns raw `HtmlFundamental` values) and the
//! shared inline engine
//! ([`InlinePatterns::render_markdown`](crate::inline::InlinePatterns::render_markdown)),
//! so prose, emphasis and links resolve exactly as on the HTML / PDF paths.
//!
//! Block-level dispatch mirrors `pdf::collect::collect_block`: diagrams (and
//! the wireframes / charts / timelines / maps nested in them) and terminals
//! render to a self-contained **static** SVG written to `_wdoc/` and referenced
//! with `![](…)`; lists, tables, code, callouts, images and math map to native
//! Markdown; videos are skipped (an online video leaves a link); everything
//! else lowers to fundamentals.

use std::fs;
use std::path::Path;

use wcl_lang::{Block, Document, Value};

use crate::build::BuildError;
use crate::inline::InlinePatterns;
use crate::render::{
    MAX_LOWER_DEPTH, as_record_variant, cell_text, collect_partials, enter_collect,
    expand_component_children, expand_repeater_children, field_bool, field_symbol, field_utf8,
    field_utf8_list, gather_inline_text, heading_level, instance_target_def, label_string,
    lower_to_values, map_list, map_utf8, map_utf8_list, render_diagram_static,
};
use crate::terminal::ASSET_DIR;

/// Render one page to a Markdown string, writing any diagram / terminal SVGs
/// into `<out_dir>/_wdoc/` as a side effect.
pub(crate) fn emit_page(
    doc: &Document,
    page: &Block<'_>,
    page_name: &str,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out_dir: &Path,
) -> Result<String, BuildError> {
    emit_page_with_front_matter(doc, page, page_name, patterns, base_dir, out_dir, None)
}

/// Like [`emit_page`], but with `front_matter` overriding the page's own
/// `@schemaless frontmatter` block when `Some` (already `---`-fenced). The
/// skill target uses this to write SKILL.md's generated front matter; `None`
/// falls back to the page's own front matter.
pub(crate) fn emit_page_with_front_matter(
    doc: &Document,
    page: &Block<'_>,
    page_name: &str,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out_dir: &Path,
    front_matter: Option<String>,
) -> Result<String, BuildError> {
    let mut em = Emitter {
        doc,
        patterns,
        base_dir,
        out_dir,
        page: page_name,
        svg_seq: 0,
    };
    let mut parts: Vec<String> = Vec::new();
    let fm = match front_matter {
        Some(fm) => Some(fm),
        None => super::yaml::front_matter(page)?,
    };
    if let Some(fm) = fm {
        // Joined below with a blank line before the first content block.
        parts.push(fm.trim_end().to_string());
    }
    for child in page.blocks() {
        em.block(&child, &mut parts)?;
    }
    let mut md = parts.join("\n\n");
    md.push('\n');
    Ok(md)
}

struct Emitter<'a> {
    doc: &'a Document,
    patterns: &'a InlinePatterns,
    base_dir: Option<&'a Path>,
    /// The site output directory; SVG assets go in `<out_dir>/_wdoc/`.
    out_dir: &'a Path,
    /// The page name, used to prefix generated SVG filenames.
    page: &'a str,
    svg_seq: usize,
}

impl Emitter<'_> {
    /// Dispatch one block, pushing zero or more complete Markdown blocks onto
    /// `out` (joined with blank lines by the caller).
    fn block(&mut self, block: &Block<'_>, out: &mut Vec<String>) -> Result<(), BuildError> {
        // `@only` / `@except` can scope a block out of this site / template /
        // backend (here, `:markdown`).
        if !crate::visibility::block_visible(block, self.patterns) {
            return Ok(());
        }
        let kind = block.kind();
        match kind {
            // Front matter is emitted separately; speaker notes never render.
            "frontmatter" | "notes" => {}
            // A presentation fragment is a step-reveal wrapper — its children
            // render in place in static output.
            "fragment" => {
                for c in block.blocks() {
                    self.block(&c, out)?;
                }
            }
            // Diagrams (and the charts / timelines / maps / tilemaps nested in
            // them) render to one self-contained static SVG.
            "diagram" => {
                let svg = render_diagram_static(self.doc, block, self.patterns, self.base_dir);
                let rel = self.write_svg("diagram", &svg)?;
                out.push(image_ref(&self.svg_alt(block, "diagram"), &rel));
            }
            "terminal" => {
                let svg = crate::terminal::render_terminal_pdf(self.doc, block, self.base_dir);
                let rel = self.write_svg("terminal", &svg)?;
                out.push(image_ref(&self.svg_alt(block, "terminal"), &rel));
            }
            "list" => out.push(self.list(block)),
            "table" => out.push(self.table(block)),
            "code" => out.push(self.code(block)),
            "image" => {
                if let Some(s) = self.image(block) {
                    out.push(s);
                }
            }
            "file" => {
                if let Some(s) = self.file(block) {
                    out.push(s);
                }
            }
            "video" => {
                if let Some(s) = self.video(block) {
                    out.push(s);
                }
            }
            "callout" => out.push(self.callout(block)),
            // A repeater stamps its body once per element of `each`; expand to
            // the bound child blocks and walk them.
            "wdoc_repeater" => {
                if block.binding_scope_depth() <= MAX_LOWER_DEPTH {
                    for c in expand_repeater_children(block) {
                        self.block(&c, out)?;
                    }
                }
            }
            // A `wdoc_instance` renders the component named by its `component`
            // value (render-by-reference); resolve the target and expand it.
            "wdoc_instance" => {
                if block.binding_scope_depth() <= MAX_LOWER_DEPTH
                    && let Some(def) = instance_target_def(block)
                {
                    self.component(block, &def, out)?;
                }
            }
            // A bare `wdoc_content` outside a component has no effect (the
            // substitution happens in `component`).
            "wdoc_content" => {}
            // A `partial` deposits tagged content for a matching `collect`; it
            // renders here only when `show_here = true`.
            "partial" => {
                if field_bool(block, "show_here") == Some(true) {
                    for c in block.blocks() {
                        self.block(&c, out)?;
                    }
                }
            }
            // A `collect` gathers every matching `partial`'s body, in document
            // order; the guard breaks collect → partial → collect cycles.
            "collect" => {
                let tag = label_string(block).unwrap_or_default();
                if let Some(_guard) = enter_collect(&tag) {
                    for c in collect_partials(self.doc, &tag) {
                        self.block(&c, out)?;
                    }
                }
            }
            kind => {
                // A user-defined `wdoc_component` instance: expand its
                // declarative body with the instance's slots bound.
                if let Some(def) = self.doc.component_def(kind) {
                    self.component(block, &def, out)?;
                } else if let Some(values) = lower_to_values(self.doc, block, kind) {
                    for v in &values {
                        self.fundamental(v, out);
                    }
                }
            }
        }
        Ok(())
    }

    /// Expand a `wdoc_component` instance: walk the definition's body with the
    /// instance's slots bound, substituting the instance's own children for a
    /// top-level `wdoc_content` placeholder (the common layout-slot case).
    fn component(
        &mut self,
        instance: &Block<'_>,
        def: &Block<'_>,
        out: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        if instance.binding_scope_depth() > MAX_LOWER_DEPTH {
            return Ok(());
        }
        for child in expand_component_children(instance, def) {
            if child.kind() == "wdoc_content" {
                for ic in instance.blocks() {
                    self.block(&ic, out)?;
                }
            } else {
                self.block(&child, out)?;
            }
        }
        Ok(())
    }

    /// Turn one lowered `HtmlFundamental` into Markdown blocks.
    fn fundamental(&self, value: &Value, out: &mut Vec<String>) {
        let Some((kind, map)) = as_record_variant(value) else {
            return;
        };
        match kind.as_str() {
            "paragraph" => {
                let text = map_utf8_list(map, "spans").join("");
                let classes = map_utf8_list(map, "class");
                if let Some(level) = heading_level(&classes) {
                    out.push(format!(
                        "{} {}",
                        "#".repeat(level as usize),
                        self.inline(&text)
                    ));
                } else {
                    self.push_para(&text, out);
                }
            }
            "element" => {
                let tag = map_utf8(map, "tag").unwrap_or_default();
                let children = map_list(map, "children");
                match tag.as_str() {
                    "p" | "span" | "div" => self.push_para(&gather_inline_text(children), out),
                    t if is_heading_tag(t) => {
                        let level = t.as_bytes()[1] - b'0';
                        out.push(format!(
                            "{} {}",
                            "#".repeat(level as usize),
                            self.inline(&gather_inline_text(children))
                        ));
                    }
                    // Unknown wrapper: descend, treating children as blocks.
                    _ => {
                        for c in children {
                            self.fundamental(c, out);
                        }
                    }
                }
            }
            "inline" => self.push_para(&map_utf8(map, "text").unwrap_or_default(), out),
            // A block equation: emit the raw LaTeX in a `$$` fence (the
            // Markdown target keeps math textual rather than rasterizing it).
            "math" => {
                let latex = map_utf8(map, "latex").unwrap_or_default();
                out.push(format!("$$\n{}\n$$", latex.trim()));
            }
            // A code block reached via `lower` (rather than the `code`-kind
            // shortcut): emit the raw source in a fenced block.
            "highlighted" => {
                let source = map_utf8(map, "source").unwrap_or_default();
                let language = map_utf8(map, "language").unwrap_or_default();
                out.push(fence(&language, &source));
            }
            // Raw HTML from a custom block — GFM allows it through verbatim.
            "raw" => {
                if let Some(html) = map_utf8(map, "html") {
                    out.push(html);
                }
            }
            _ => {}
        }
    }

    /// Run inline text through the shared pattern engine, in Markdown mode.
    fn inline(&self, text: &str) -> String {
        self.patterns.render_markdown(self.doc, text)
    }

    /// Push a paragraph unless it renders empty.
    fn push_para(&self, text: &str, out: &mut Vec<String>) {
        let s = self.inline(text);
        if !s.trim().is_empty() {
            out.push(s);
        }
    }

    /// Write an SVG to `<out_dir>/_wdoc/<page>-<kind>-<n>.svg` and return the
    /// page-relative reference (`_wdoc/…`).
    fn write_svg(&mut self, kind: &str, svg: &str) -> Result<String, BuildError> {
        self.svg_seq += 1;
        let file = format!("{}-{kind}-{}.svg", sanitize(self.page), self.svg_seq);
        let dir = self.out_dir.join(ASSET_DIR);
        fs::create_dir_all(&dir)
            .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
        let path = dir.join(&file);
        fs::write(&path, ensure_svg_namespace(svg))
            .map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
        // The SVG lands in `<root>/_wdoc/`; a skill reference page (one level
        // deep) references it through `../`.
        Ok(format!(
            "{}{ASSET_DIR}/{file}",
            self.patterns.asset_prefix()
        ))
    }

    /// Alt text for a generated SVG: the block's `title`, else its `id`, else
    /// the kind.
    fn svg_alt(&self, block: &Block<'_>, kind: &str) -> String {
        field_utf8(block, "title")
            .or_else(|| field_utf8(block, "id"))
            .unwrap_or_else(|| kind.to_string())
    }

    /// A bullet / numbered list, flattened to indented Markdown lines.
    fn list(&self, block: &Block<'_>) -> String {
        let mut lines = Vec::new();
        let ordered = field_symbol(block, "style").as_deref() == Some("numbered");
        self.li_group(block, ordered, 0, &mut lines);
        lines.join("\n")
    }

    fn li_group(&self, parent: &Block<'_>, ordered: bool, depth: usize, lines: &mut Vec<String>) {
        let mut i = 0u32;
        for li in parent.blocks().filter(|b| b.kind() == "li") {
            i += 1;
            let indent = "  ".repeat(depth);
            let marker = if ordered {
                format!("{i}.")
            } else {
                "-".to_string()
            };
            let text = self.inline(&label_string(&li).unwrap_or_default());
            lines.push(format!("{indent}{marker} {text}"));

            // Implicit sublist: bare `li`s directly under this `li`.
            if li.blocks().any(|b| b.kind() == "li") {
                self.li_group(&li, ordered, depth + 1, lines);
            }
            // Explicit nested `list` blocks with their own style.
            for sub in li.blocks().filter(|b| b.kind() == "list") {
                let sub_ordered = field_symbol(&sub, "style").as_deref() == Some("numbered");
                self.li_group(&sub, sub_ordered, depth + 1, lines);
            }
        }
    }

    /// A table: header row + separator + body rows, each cell run through the
    /// inline engine. Reads the computed-`rows` form first, then the native
    /// pipe-row form (whose first row is the header).
    fn table(&self, block: &Block<'_>) -> String {
        let mut header: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();

        if let Some(Value::List(body)) = block.field("rows").and_then(|f| f.value().ok().cloned()) {
            if let Some(Value::List(cells)) =
                block.field("header").and_then(|f| f.value().ok().cloned())
            {
                header = cells.iter().map(|v| self.cell(v)).collect();
            }
            for r in &body {
                match r {
                    Value::List(cells) => rows.push(cells.iter().map(|v| self.cell(v)).collect()),
                    other => rows.push(vec![self.cell(other)]),
                }
            }
        } else {
            let mut all: Vec<Vec<String>> = Vec::new();
            for table in block.tables() {
                for row in table.rows() {
                    if let Ok(values) = row.values() {
                        all.push(values.iter().map(|v| self.cell(v)).collect());
                    }
                }
            }
            if !all.is_empty() {
                header = all.remove(0);
            }
            rows = all;
        }
        render_pipe_table(&header, &rows)
    }

    fn cell(&self, v: &Value) -> String {
        escape_cell(&self.inline(&cell_text(v)))
    }

    /// A syntax-tagged code block: language from the inline label, source from
    /// the `source` field, in a fenced block (no highlighting — Markdown
    /// renderers re-tokenize from the language tag).
    fn code(&self, block: &Block<'_>) -> String {
        let lang = label_string(block).unwrap_or_default();
        let source = field_utf8(block, "source").unwrap_or_default();
        fence(&lang, &source)
    }

    /// A page image: register the source (local files are copied to `_wdoc/`
    /// after the page loop) and reference its resolved URL.
    fn image(&self, block: &Block<'_>) -> Option<String> {
        let source = label_string(block)?;
        let entry = self.patterns.images().register(&source);
        let alt = field_utf8(block, "alt").unwrap_or_default();
        Some(image_ref(&alt, &self.asset_href(&entry.url)))
    }

    /// Prefix a root-relative asset URL with the current page's `../` depth
    /// (skill reference pages). External URLs pass through unchanged.
    fn asset_href(&self, url: &str) -> String {
        let prefix = self.patterns.asset_prefix();
        if prefix.is_empty() || crate::image::is_external(url) {
            url.to_string()
        } else {
            format!("{prefix}{url}")
        }
    }

    /// A `file` block: register the source (copied into its `dir` after the
    /// page loop) and, when `as` (link text) is set, emit a Markdown link to
    /// its path. Absent `as` ⇒ the file is shipped silently (no output).
    fn file(&self, block: &Block<'_>) -> Option<String> {
        let source = label_string(block)?;
        let dir = field_utf8(block, "dir").unwrap_or_default();
        let entry = self.patterns.files().register(&source, &dir);
        let text = field_utf8(block, "as")?;
        Some(format!(
            "[{}]({})",
            escape_link_text(&text),
            self.asset_href(&entry.url)
        ))
    }

    /// A page video: an online video becomes a plain link; a local video is
    /// dropped (a static Markdown file can't play it).
    fn video(&self, block: &Block<'_>) -> Option<String> {
        let source = label_string(block)?;
        let url = crate::video::online_url(&source)?;
        let title = field_utf8(block, "title").unwrap_or_else(|| url.clone());
        Some(format!("[{}]({url})", escape_link_text(&title)))
    }

    /// A callout → a GitHub-style alert blockquote.
    fn callout(&self, block: &Block<'_>) -> String {
        let classes = field_utf8_list(block, "class");
        let heading = self.inline(&label_string(block).unwrap_or_default());
        let body = self.inline(&field_utf8(block, "body").unwrap_or_default());
        let mut lines = vec![format!("> [!{}]", callout_alert(&classes))];
        if !heading.trim().is_empty() {
            lines.push(format!("> **{heading}**"));
        }
        for line in body.split('\n') {
            lines.push(format!("> {line}"));
        }
        lines.join("\n")
    }
}

/// A Markdown image reference, alt text lightly escaped.
fn image_ref(alt: &str, url: &str) -> String {
    format!("![{}]({url})", escape_link_text(alt))
}

/// Map a callout's type class to a GitHub alert keyword.
fn callout_alert(classes: &[String]) -> &'static str {
    for c in classes {
        match c.as_str() {
            "note" | "info" => return "NOTE",
            "tip" | "success" => return "TIP",
            "warning" => return "WARNING",
            "error" => return "CAUTION",
            _ => {}
        }
    }
    "NOTE"
}

/// Build a GitHub-flavoured pipe table from header + body cells. Width is the
/// widest row; short rows pad with empty cells.
fn render_pipe_table(header: &[String], rows: &[Vec<String>]) -> String {
    let cols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return String::new();
    }
    let row_line = |cells: &[String]| -> String {
        let padded: Vec<String> = (0..cols)
            .map(|i| cells.get(i).cloned().unwrap_or_default())
            .collect();
        format!("| {} |", padded.join(" | "))
    };
    let mut out = String::new();
    out.push_str(&row_line(header));
    out.push('\n');
    out.push_str(&format!("|{}", " --- |".repeat(cols)));
    for r in rows {
        out.push('\n');
        out.push_str(&row_line(r));
    }
    out
}

/// A fenced code block, widening the fence past any backtick run in the
/// source so embedded triple-backticks survive.
fn fence(lang: &str, source: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(source).max(2) + 1);
    let src = source.strip_suffix('\n').unwrap_or(source);
    format!("{ticks}{lang}\n{src}\n{ticks}")
}

fn longest_backtick_run(s: &str) -> usize {
    let mut longest = 0usize;
    let mut run = 0usize;
    for ch in s.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Ensure a standalone SVG carries the SVG namespace so it renders when
/// referenced as an image file. Diagram SVGs already do; terminal SVGs get
/// it injected defensively if missing.
fn ensure_svg_namespace(svg: &str) -> String {
    if svg.contains("xmlns=") || !svg.trim_start().starts_with("<svg") {
        return svg.to_string();
    }
    svg.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1)
}

/// Escape characters that would break Markdown link / image text.
fn escape_link_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('\n', " ")
}

/// Escape a table cell: pipes are literal, newlines collapse to a space.
fn escape_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn is_heading_tag(tag: &str) -> bool {
    matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// Sanitize a page name into a filename-safe stem.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

