//! Printing string literals: escaping, encoding prefixes, interpolation
//! and the heredoc forms.
//!
//! The heredoc decision is the interesting part. A multi-line body reads
//! far better as a heredoc, but only if it round-trips — so
//! [`heredoc_round_trips`] checks that first, and
//! [`pick_heredoc_tag`] picks a tag the body cannot contain.

use crate::ast::TemplatePart;
use crate::lexer::StringEncoding;
use crate::value::EscapeString;

use super::Printer;

impl Printer {
    /// Print a string literal, choosing a heredoc when that reads better.
    pub(super) fn print_string_lit(&mut self, body: &str, encoding: StringEncoding) {
        self.print_string_lit_in(body, encoding, false);
    }

    /// Print a string literal with an explicit say over whether the
    /// heredoc form is allowed — nested positions forbid it.
    pub(super) fn print_string_lit_in(
        &mut self,
        body: &str,
        encoding: StringEncoding,
        allow_heredoc: bool,
    ) {
        // Heredoc form only where it is *legal* (`allow_heredoc`: a
        // statement-level value followed by a bare newline), *value-
        // preserving* (`heredoc_round_trips`: re-parsing the emitted
        // heredoc reproduces the body exactly), and *worth it* (two or
        // more lines — `"\n"`-ish separator strings stay escaped
        // literals). Everything else prints as a quoted literal with
        // escapes, which always round-trips.
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        if allow_heredoc && heredoc_round_trips(body) && body.lines().count() >= 2 {
            self.print_heredoc(body, prefix, false);
        } else {
            self.push(prefix);
            self.push("\"");
            self.push(&EscapeString(body).to_string());
            self.push("\"");
        }
    }

    /// Print an interpolated literal, re-emitting each `${…}` slot.
    pub(super) fn print_interpolated(
        &mut self,
        encoding: StringEncoding,
        parts: &[TemplatePart],
        allow_heredoc: bool,
    ) {
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        // A "skeleton" of the body — literal text with each `${…}` slot
        // standing in as a single placeholder character — drives the
        // style choice: the heredoc form is used only where it is legal
        // (`allow_heredoc`), the body round-trips through heredoc
        // line/indent handling, the *last* part ends with a newline (the
        // closing tag must start its own line — a body that was
        // authored with `\n` escapes and doesn't end on one cannot be a
        // heredoc), and the text is genuinely multi-line.
        let mut skeleton = String::new();
        for part in parts {
            match part {
                TemplatePart::Literal(s) => skeleton.push_str(s),
                TemplatePart::Expr(_) => skeleton.push('x'),
            }
        }
        let ends_on_newline = matches!(
            parts.last(),
            Some(TemplatePart::Literal(s)) if s.ends_with('\n')
        );
        self.push("$");
        self.push(prefix);
        if allow_heredoc
            && ends_on_newline
            && heredoc_round_trips(&skeleton)
            && skeleton.lines().count() >= 2
        {
            // Pick a tag no body line could close early on ("INTERP"
            // first to keep existing formatted files stable).
            let tag = pick_heredoc_tag_preferring(&skeleton, "INTERP");
            self.push("<<");
            self.push(&tag);
            self.push("\n");
            for part in parts {
                match part {
                    // The interpolated heredoc body is escape-
                    // interpreted: a literal backslash must double and a
                    // literal `${` must re-escape to `\${`, or the text
                    // re-parses as an escape / a slot.
                    TemplatePart::Literal(s) => {
                        self.push(&s.replace('\\', "\\\\").replace("${", "\\${"));
                    }
                    TemplatePart::Expr(e) => {
                        self.push("${");
                        self.slot_depth += 1;
                        self.print_expr(e, 0);
                        self.slot_depth -= 1;
                        self.push("}");
                    }
                }
            }
            // The final literal ends with `\n` (checked above), so the
            // closing tag starts its own line. Don't add another — that
            // would creep one extra blank line in on every reformat.
            self.push(&tag);
        } else {
            self.push("\"");
            for part in parts {
                match part {
                    // `EscapeString` covers quotes / backslashes /
                    // control characters but not `${` — in an
                    // *interpolated* literal that sequence must
                    // re-escape to `\${` or it re-parses as a slot.
                    // (Safe after EscapeString: it never produces `${`
                    // from other characters.)
                    TemplatePart::Literal(s) => {
                        self.push(&EscapeString(s).to_string().replace("${", "\\${"));
                    }
                    TemplatePart::Expr(e) => {
                        self.push("${");
                        self.slot_depth += 1;
                        self.print_expr(e, 0);
                        self.slot_depth -= 1;
                        self.push("}");
                    }
                }
            }
            self.push("\"");
        }
    }

    /// Print a heredoc body under the given tag prefix.
    pub(super) fn print_heredoc(&mut self, body: &str, prefix: &str, _interpolated: bool) {
        // Indent each body line at depth + 1 so parse-time indent
        // stripping recovers the original content. The trailing newline
        // is significant — the parser adds one per line, so the
        // round-trip value ends with `\n`.
        let body_indent = self.indent_str.repeat((self.depth + 1) as usize);
        let closer_indent = self.indent_str.repeat(self.depth as usize);
        // Pick a tag that doesn't collide with a (trimmed) body line,
        // so the closer can't trigger early.
        let tag = pick_heredoc_tag(body);

        // A plain `<<TAG` body is escape-interpreted on re-parse, so a
        // backslash would break the round-trip (`\f` → invalid escape).
        // For utf8 bodies emit a raw `<<'TAG'` heredoc — body taken
        // verbatim, which also keeps backslash-heavy text (LaTeX,
        // regexes) readable. The rarer typed-encoding heredocs fall back
        // to a plain heredoc with backslashes escaped so the value still
        // round-trips.
        if prefix.is_empty() && body.contains('\\') {
            self.push("<<'");
            self.push(&tag);
            self.push("'\n");
            for line in body.split_inclusive('\n') {
                self.push(&body_indent);
                self.push(line);
            }
            self.push(&closer_indent);
            self.push(&tag);
            return;
        }

        self.push(prefix);
        self.push("<<");
        self.push(&tag);
        self.push("\n");
        for line in body.split_inclusive('\n') {
            self.push(&body_indent);
            // Escape backslashes only; the literal `\n` stays a line break.
            self.push(&line.replace('\\', "\\\\"));
        }
        self.push(&closer_indent);
        self.push(&tag);
    }
}

/// Greedy-wrap single-line prose at `width` columns, breaking only at
/// spaces that sit outside inline-markup constructs, and return the
/// heredoc-shaped body (lines joined with `\n`, trailing `\n`).
///
/// The stdlib inline patterns (bold / italic / code / link / math)
/// deliberately don't match across `\n`, so a break inside `**…**` or
/// `[…](…)` would change rendering. The scanner below marks those spans
/// unbreakable; a false positive (e.g. two incidental underscores) only
/// costs a break opportunity, never correctness. A word longer than
/// `width` (a URL) is left on its own over-long line rather than split.
pub(super) fn wrap_prose(text: &str, width: usize) -> String {
    let breaks = safe_break_points(text);
    let cols: Vec<usize> = {
        // Byte index → visual column (chars before it), for width checks.
        let mut v = vec![0; text.len() + 1];
        for (col, (i, c)) in text.char_indices().enumerate() {
            v[i] = col;
            for b in 1..c.len_utf8() {
                v[i + b] = col;
            }
        }
        v[text.len()] = text.chars().count();
        v
    };

    let mut out = String::with_capacity(text.len() + 8);
    let mut start = 0usize;
    while start < text.len() {
        let line_end_col = cols[start] + width;
        // The last safe break within the width, else the first one past it.
        let within = breaks
            .iter()
            .filter(|&&b| b > start && cols[b] <= line_end_col)
            .max()
            .copied();
        let past = breaks.iter().filter(|&&b| b > start).min().copied();
        let cut = match (cols[text.len()] <= line_end_col, within, past) {
            (true, ..) => None, // the rest fits
            (false, Some(b), _) | (false, None, Some(b)) => Some(b),
            (false, None, None) => None,
        };
        match cut {
            Some(b) => {
                out.push_str(text[start..b].trim_end());
                out.push('\n');
                start = b + 1; // consume the break space…
                while text.as_bytes().get(start) == Some(&b' ') {
                    start += 1; // …and any run of spaces after it
                }
            }
            None => {
                out.push_str(text[start..].trim_end());
                out.push('\n');
                break;
            }
        }
    }
    out
}

/// Byte offsets of the spaces in `text` where a line break is safe:
/// outside inline code spans, bold/italic runs, links, and math — and
/// not after a `>` (the blockquote pattern styles to end-of-line, so a
/// break would move where the quote ends).
pub(super) fn safe_break_points(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut in_code = false; // `…`
    let mut in_bold = false; // **…**
    let mut in_italic = false; // _…_
    let mut in_math = false; // $…$ / $$…$$
    let mut link_depth = 0u32; // […](…) — [ to the closing )
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'`' => in_code = !in_code,
            _ if in_code => {} // a code span hides every other marker
            b'>' => break,     // nothing after a `>` is a safe break
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                in_bold = !in_bold;
                i += 1;
            }
            b'_' if text[i + 1..].contains('_') || in_italic => in_italic = !in_italic,
            b'$' => in_math = !in_math,
            b'[' => link_depth += 1,
            // `](` continues the link into its target; a bare `]` ends it.
            b']' if link_depth > 0 && bytes.get(i + 1) != Some(&b'(') => link_depth -= 1,
            b')' if link_depth > 0 => link_depth -= 1,
            b' ' if !in_bold && !in_italic && !in_math && link_depth == 0 => out.push(i),
            _ => {}
        }
        i += 1;
    }
    out
}

/// `true` when `body` would survive a heredoc round-trip exactly.
/// Heredoc parsing imposes three constraints a quoted literal doesn't:
///
/// - every body line contributes `content + '\n'`, so a value that
///   doesn't end with a newline is unrepresentable (the closing tag
///   would glue onto the last line and never close);
/// - the minimum leading whitespace across non-blank lines is stripped
///   from every line, so a body whose lines *all* start with whitespace
///   loses it on re-parse;
/// - whitespace-only lines are blanked entirely, so a line of spaces
///   loses them.
pub(super) fn heredoc_round_trips(body: &str) -> bool {
    if !body.ends_with('\n') {
        return false;
    }
    let mut any_nonblank = false;
    let mut any_zero_indent = false;
    for line in body.lines() {
        if line.trim().is_empty() {
            if !line.is_empty() {
                // Whitespace-only line: blanked on re-parse.
                return false;
            }
        } else {
            any_nonblank = true;
            if !line.starts_with([' ', '\t']) {
                any_zero_indent = true;
            }
        }
    }
    !any_nonblank || any_zero_indent
}

/// [`pick_heredoc_tag`] with a preferred first candidate (the
/// interpolated form keeps its historical `INTERP` tag when possible).
pub(super) fn pick_heredoc_tag_preferring(body: &str, preferred: &str) -> String {
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    if !lines.contains(&preferred) {
        return preferred.to_string();
    }
    pick_heredoc_tag(body)
}

/// Choose a heredoc tag that no (trimmed) body line equals, so the
/// closer line can't fire early. Falls back to a numbered tag in the
/// pathological case where every candidate appears in the body.
pub(super) fn pick_heredoc_tag(body: &str) -> String {
    let lines: std::collections::HashSet<&str> = body.lines().map(str::trim).collect();
    for cand in ["DOC", "TEX", "RAW", "MATH", "BODY", "END", "HEREDOC"] {
        if !lines.contains(cand) {
            return cand.to_string();
        }
    }
    let mut i = 0;
    loop {
        let t = format!("DOC{i}");
        if !lines.contains(t.as_str()) {
            return t;
        }
        i += 1;
    }
}
