//! Path and glob builtins: `path_contains` (segment-aware prefix test),
//! `glob_match` (one concrete path against a glob), and `glob_overlaps`
//! (do two glob patterns share any possible path). Registered in
//! [`Environment::new`](crate::Environment::new).
//!
//! Paths are treated as `/`-separated segment sequences; empty and `.`
//! segments are dropped, so `src/`, `src`, and `./src` all normalise to
//! the same path. Glob syntax: `*` matches any run of characters within
//! one segment, `?` matches one character, `[abc]` / `[a-z]` / `[!x]`
//! character classes, and a `**` segment matches zero or more whole
//! segments. A pattern with a trailing `/` owns the whole subtree — it is
//! read as `pattern/**`.

use crate::builtins::from_fn;
use crate::environment::Environment;

/// Register every path/glob builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "path_contains",
        from_fn(|parent: String, child: String| -> bool { path_contains(&parent, &child) })
            .doc(
                "Segment-aware path prefix test: whether `child` is `parent` itself or lives \
                 under it. Splits on `/`, so `src/` does not contain `src2/x`. A path contains \
                 itself.",
            )
            .param(
                "parent",
                "utf8",
                "The containing path (trailing slash optional).",
            )
            .param("child", "utf8", "The path to test.")
            .returns(
                "bool",
                "`true` if `child` equals `parent` or is nested beneath it.",
            ),
    );
    env.add_builtin(
        "glob_match",
        from_fn(|pattern: String, path: String| -> bool { glob_match(&pattern, &path) })
            .doc(
                "Match one concrete path against a glob. `*` stays within a segment, `**` \
                 spans segments, `?` matches one character, `[a-z]` / `[!x]` are character \
                 classes. A trailing `/` on the pattern matches the whole subtree.",
            )
            .param("pattern", "utf8", "The glob pattern.")
            .param("path", "utf8", "The concrete path to test.")
            .returns("bool", "`true` if the path matches the pattern."),
    );
    env.add_builtin(
        "glob_overlaps",
        from_fn(|a: String, b: String| -> bool { glob_overlaps(&a, &b) })
            .doc(
                "Whether two glob patterns can match a common path. Concrete paths are \
                 patterns too, so this subsumes `glob_match` for overlap gates. Trailing `/` \
                 means the whole subtree. Conservative: exotic negated-class pairings may \
                 report `true` when no shared path exists, never `false` when one does.",
            )
            .param("a", "utf8", "The first glob pattern (or concrete path).")
            .param("b", "utf8", "The second glob pattern (or concrete path).")
            .returns("bool", "`true` if some path is matched by both patterns."),
    );
}

/// Split a path into normalised segments: empty and `.` segments dropped.
fn segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect()
}

fn path_contains(parent: &str, child: &str) -> bool {
    let p = segments(parent);
    let c = segments(child);
    p.len() <= c.len() && p.iter().zip(&c).all(|(a, b)| a == b)
}

// ─── Glob pattern representation ─────────────────────────────────────

/// One item of a `[...]` character class.
#[derive(Debug, Clone, PartialEq)]
enum ClassItem {
    Ch(char),
    Range(char, char),
}

/// A single-character token inside one pattern segment.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// `?` — any one character.
    Any,
    Lit(char),
    /// `[...]` — a (possibly negated) character class.
    Class {
        neg: bool,
        items: Vec<ClassItem>,
    },
}

/// One element of a pattern segment: a `*` wildcard or a one-character token.
#[derive(Debug, Clone, PartialEq)]
enum PatTok {
    Star,
    Tok(Tok),
}

/// One `/`-separated element of a glob pattern.
#[derive(Debug, Clone, PartialEq)]
enum Seg {
    /// A bare `**` segment: zero or more whole path segments.
    Globstar,
    Pat(Vec<PatTok>),
}

/// Parse a glob pattern into segments. A trailing `/` appends a `Globstar`
/// (subtree ownership).
fn parse_glob(pattern: &str) -> Vec<Seg> {
    let subtree = pattern.ends_with('/') && !segments(pattern).is_empty();
    let mut segs: Vec<Seg> = segments(pattern).into_iter().map(parse_segment).collect();
    if subtree {
        segs.push(Seg::Globstar);
    }
    segs
}

fn parse_segment(seg: &str) -> Seg {
    if seg == "**" {
        return Seg::Globstar;
    }
    let chars: Vec<char> = seg.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                // Collapse runs of `*` into one; `**` inside a segment is
                // just `*` (only a whole `**` segment spans segments).
                if toks.last() != Some(&PatTok::Star) {
                    toks.push(PatTok::Star);
                }
                i += 1;
            }
            '?' => {
                toks.push(PatTok::Tok(Tok::Any));
                i += 1;
            }
            '[' => match parse_class(&chars, i) {
                Some((tok, next)) => {
                    toks.push(PatTok::Tok(tok));
                    i = next;
                }
                // Unterminated class: treat the `[` as a literal.
                None => {
                    toks.push(PatTok::Tok(Tok::Lit('[')));
                    i += 1;
                }
            },
            c => {
                toks.push(PatTok::Tok(Tok::Lit(c)));
                i += 1;
            }
        }
    }
    Seg::Pat(toks)
}

/// Parse a `[...]` class starting at `chars[open] == '['`. Returns the token
/// and the index just past the closing `]`, or `None` when unterminated.
/// The first content character (after an optional `!`/`^`) is literal even
/// if it is `]`.
fn parse_class(chars: &[char], open: usize) -> Option<(Tok, usize)> {
    let mut i = open + 1;
    let neg = matches!(chars.get(i), Some('!') | Some('^'));
    if neg {
        i += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    while i < chars.len() {
        let c = chars[i];
        if c == ']' && !first {
            return Some((Tok::Class { neg, items }, i + 1));
        }
        first = false;
        if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some_and(|&e| e != ']') {
            items.push(ClassItem::Range(c, chars[i + 2]));
            i += 3;
        } else {
            items.push(ClassItem::Ch(c));
            i += 1;
        }
    }
    None
}

// ─── Matching a concrete path ────────────────────────────────────────

fn glob_match(pattern: &str, path: &str) -> bool {
    let pat = parse_glob(pattern);
    let segs: Vec<Vec<char>> = segments(path).iter().map(|s| s.chars().collect()).collect();
    segs_match(&pat, &segs)
}

fn segs_match(pat: &[Seg], path: &[Vec<char>]) -> bool {
    match pat.split_first() {
        None => path.is_empty(),
        Some((Seg::Globstar, rest)) => {
            segs_match(rest, path) || (!path.is_empty() && segs_match(pat, &path[1..]))
        }
        Some((Seg::Pat(p), rest)) => match path.split_first() {
            Some((seg, path_rest)) => seg_match(p, seg) && segs_match(rest, path_rest),
            None => false,
        },
    }
}

fn seg_match(pat: &[PatTok], text: &[char]) -> bool {
    match pat.split_first() {
        None => text.is_empty(),
        Some((PatTok::Star, rest)) => (0..=text.len()).any(|k| seg_match(rest, &text[k..])),
        Some((PatTok::Tok(t), rest)) => match text.split_first() {
            Some((&c, text_rest)) => tok_matches(t, c) && seg_match(rest, text_rest),
            None => false,
        },
    }
}

fn tok_matches(t: &Tok, c: char) -> bool {
    match t {
        Tok::Any => true,
        Tok::Lit(l) => *l == c,
        Tok::Class { neg, items } => {
            let hit = items.iter().any(|it| match it {
                ClassItem::Ch(x) => *x == c,
                ClassItem::Range(lo, hi) => (*lo..=*hi).contains(&c),
            });
            hit != *neg
        }
    }
}

// ─── Pattern-vs-pattern overlap ──────────────────────────────────────

fn glob_overlaps(a: &str, b: &str) -> bool {
    segs_intersect(&parse_glob(a), &parse_glob(b))
}

fn segs_intersect(a: &[Seg], b: &[Seg]) -> bool {
    match (a.split_first(), b.split_first()) {
        (None, None) => true,
        (Some((Seg::Globstar, a_rest)), _) => {
            // The globstar spans zero segments…
            if segs_intersect(a_rest, b) {
                return true;
            }
            // …or produces one more segment that b's head also covers.
            match b.split_first() {
                Some((Seg::Globstar, b_rest)) => segs_intersect(a, b_rest),
                Some((Seg::Pat(p), b_rest)) => pat_nonempty(p) && segs_intersect(a, b_rest),
                None => false,
            }
        }
        (_, Some((Seg::Globstar, _))) => segs_intersect(b, a),
        (None, Some(_)) | (Some(_), None) => false,
        (Some((Seg::Pat(x), a_rest)), Some((Seg::Pat(y), b_rest))) => {
            pats_intersect(x, y) && segs_intersect(a_rest, b_rest)
        }
    }
}

/// Can two segment patterns match a common string? Total call weight is
/// bounded by `len(a) + len(b)` per step, shrinking every recursion.
fn pats_intersect(a: &[PatTok], b: &[PatTok]) -> bool {
    match (a.split_first(), b.split_first()) {
        (None, None) => true,
        (Some((PatTok::Star, a_rest)), _) => {
            // The star matches the empty string…
            if pats_intersect(a_rest, b) {
                return true;
            }
            // …or emits one more character that b's head also matches
            // (a star matches any character, so b's head only needs to
            // accept *some* character).
            match b.split_first() {
                Some((PatTok::Star, b_rest)) => pats_intersect(a, b_rest),
                Some((PatTok::Tok(t), b_rest)) => tok_nonempty(t) && pats_intersect(a, b_rest),
                None => false,
            }
        }
        (_, Some((PatTok::Star, _))) => pats_intersect(b, a),
        (None, Some(_)) | (Some(_), None) => false,
        (Some((PatTok::Tok(x), a_rest)), Some((PatTok::Tok(y), b_rest))) => {
            toks_intersect(x, y) && pats_intersect(a_rest, b_rest)
        }
    }
}

/// Does the token accept at least one character?
fn tok_nonempty(t: &Tok) -> bool {
    match t {
        Tok::Any | Tok::Lit(_) => true,
        // A negated class always excludes finitely many characters, so it
        // accepts something; a positive class needs at least one item.
        Tok::Class { neg, items } => *neg || !items.is_empty(),
    }
}

/// Does the segment pattern match at least one string?
fn pat_nonempty(p: &[PatTok]) -> bool {
    p.iter().all(|t| match t {
        PatTok::Star => true,
        PatTok::Tok(t) => tok_nonempty(t),
    })
}

/// Do two single-character tokens accept a common character? Exact except
/// for class-vs-class pairings involving negation, which conservatively
/// report `true` (safe for overlap gates: never a false "disjoint").
fn toks_intersect(a: &Tok, b: &Tok) -> bool {
    match (a, b) {
        (Tok::Any, other) | (other, Tok::Any) => tok_nonempty(other),
        (Tok::Lit(x), Tok::Lit(y)) => x == y,
        (Tok::Lit(c), cls @ Tok::Class { .. }) | (cls @ Tok::Class { .. }, Tok::Lit(c)) => {
            tok_matches(cls, *c)
        }
        (
            Tok::Class {
                neg: false,
                items: xs,
            },
            Tok::Class {
                neg: false,
                items: ys,
            },
        ) => xs
            .iter()
            .any(|x| ys.iter().any(|y| class_items_overlap(x, y))),
        // At least one side negated: almost always overlapping; being exact
        // needs full character-set subtraction for no practical gain.
        (Tok::Class { .. }, Tok::Class { .. }) => true,
    }
}

fn class_items_overlap(a: &ClassItem, b: &ClassItem) -> bool {
    let (alo, ahi) = match a {
        ClassItem::Ch(c) => (*c, *c),
        ClassItem::Range(lo, hi) => (*lo, *hi),
    };
    let (blo, bhi) = match b {
        ClassItem::Ch(c) => (*c, *c),
        ClassItem::Range(lo, hi) => (*lo, *hi),
    };
    alo <= bhi && blo <= ahi
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_contains_segment_aware() {
        assert!(path_contains("src/", "src/core/mod.rs"));
        assert!(path_contains("src", "src/core"));
        assert!(!path_contains("src/", "src2/x"));
        assert!(!path_contains("src/core", "src"));
        assert!(path_contains("src/", "src"));
        assert!(path_contains("./src", "src/x"));
        assert!(path_contains("", "anything/at/all"));
    }

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("src/*.rs", "src/main.rs"));
        assert!(!glob_match("src/*.rs", "src/sub/mod.rs"));
        assert!(!glob_match("src/*.rs", "src/main.md"));
        assert!(glob_match("src/**", "src/a/b/c.rs"));
        assert!(glob_match("src/**", "src"));
        assert!(glob_match("**/*.rs", "a/b/c.rs"));
        assert!(!glob_match("**/*.rs", "c.md"));
        assert!(glob_match("a/?.rs", "a/x.rs"));
        assert!(!glob_match("a/?.rs", "a/xy.rs"));
        assert!(glob_match("[a-c].rs", "b.rs"));
        assert!(!glob_match("[a-c].rs", "d.rs"));
        assert!(glob_match("[!a-c].rs", "d.rs"));
        assert!(!glob_match("[!a-c].rs", "b.rs"));
    }

    #[test]
    fn glob_match_trailing_slash_is_subtree() {
        assert!(glob_match("src/", "src/core/mod.rs"));
        assert!(glob_match("src/", "src"));
        assert!(!glob_match("src/", "src2/x"));
    }

    #[test]
    fn glob_match_unterminated_class_is_literal() {
        assert!(glob_match("a[bc", "a[bc"));
        assert!(!glob_match("a[bc", "ab"));
    }

    #[test]
    fn overlap_exact_and_disjoint() {
        assert!(glob_overlaps("src/main.rs", "src/main.rs"));
        assert!(!glob_overlaps("src/main.rs", "src/lib.rs"));
        assert!(!glob_overlaps("src/*.rs", "docs/*.md"));
        assert!(!glob_overlaps("*.rs", "*.md"));
    }

    #[test]
    fn overlap_glob_vs_concrete() {
        assert!(glob_overlaps("src/*.rs", "src/main.rs"));
        assert!(glob_overlaps("src/main.rs", "src/*.rs"));
        assert!(!glob_overlaps("src/*.rs", "src/sub/mod.rs"));
    }

    #[test]
    fn overlap_dir_prefix() {
        assert!(glob_overlaps("src/", "src/core/"));
        assert!(glob_overlaps("src/core/", "src/"));
        assert!(!glob_overlaps("src/", "src2/"));
        assert!(glob_overlaps("src/", "src/*.rs"));
        assert!(glob_overlaps("src/**", "src/core/x.rs"));
    }

    #[test]
    fn overlap_star_combinations() {
        assert!(glob_overlaps("a/*/c", "a/b/*"));
        assert!(glob_overlaps("a/*", "a/b"));
        assert!(!glob_overlaps("a/*/c", "a/b/d"));
        assert!(glob_overlaps("**/x.rs", "src/**"));
        assert!(glob_overlaps("*.rs", "main.*"));
        assert!(!glob_overlaps("a*.rs", "b*.rs"));
        assert!(glob_overlaps("a*b", "ab"));
        assert!(!glob_overlaps("a*b", "ac"));
    }

    #[test]
    fn overlap_classes() {
        assert!(glob_overlaps("[a-c].rs", "[c-e].rs"));
        assert!(!glob_overlaps("[a-c].rs", "[d-e].rs"));
        assert!(glob_overlaps("[a-c].rs", "b.rs"));
        assert!(!glob_overlaps("[a-c].rs", "d.rs"));
        // Negated classes are conservatively overlapping.
        assert!(glob_overlaps("[!a].rs", "[!b].rs"));
    }

    #[test]
    fn overlap_globstar_edges() {
        assert!(glob_overlaps("**", "anything/here"));
        assert!(glob_overlaps("**", "**"));
        assert!(glob_overlaps("src/**/*.rs", "src/deep/nest/main.rs"));
        assert!(!glob_overlaps("src/**/*.rs", "src/deep/nest/main.md"));
    }
}
