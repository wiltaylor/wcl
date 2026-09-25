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

use crate::environment::Environment;
use crate::functions::from_fn;

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

/// Whether `child` sits at or beneath `parent`, comparing whole
/// segments so `/ab` does not contain `/abc`.
fn path_contains(parent: &str, child: &str) -> bool {
    let p = segments(parent);
    let c = segments(child);
    p.len() <= c.len() && p.iter().zip(&c).all(|(a, b)| a == b)
}

// ─── Glob pattern representation ─────────────────────────────────────

/// One item of a `[...]` character class.
#[derive(Debug, Clone, PartialEq)]
enum ClassItem {
    /// A single literal character.
    Ch(char),
    /// An inclusive character range.
    Range(char, char),
}

/// A single-character token inside one pattern segment.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// `?` — any one character.
    Any,
    /// A literal character.
    Lit(char),
    /// `[...]` — a (possibly negated) character class.
    Class {
        /// Whether the class was negated with a leading `!`.
        neg: bool,
        /// The characters and ranges the class lists.
        items: Vec<ClassItem>,
    },
}

/// One element of a pattern segment: a `*` wildcard or a one-character token.
#[derive(Debug, Clone, PartialEq)]
enum PatTok {
    /// A `**` segment, matching any number of segments.
    Star,
    /// A single-character matcher.
    Tok(Tok),
}

/// One `/`-separated element of a glob pattern.
#[derive(Debug, Clone, PartialEq)]
enum Seg {
    /// A bare `**` segment: zero or more whole path segments.
    Globstar,
    /// A segment matched token by token.
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

/// Compile one glob segment into its matcher form.
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

/// Whether `path` matches the glob `pattern`.
fn glob_match(pattern: &str, path: &str) -> bool {
    let pat = parse_glob(pattern);
    let segs: Vec<Vec<char>> = segments(path).iter().map(|s| s.chars().collect()).collect();
    segs_match(&pat, &segs)
}

/// Match compiled segments against a split path; a `**` segment
/// consumes any number of path segments.
fn segs_match(pat: &[Seg], path: &[Vec<char>]) -> bool {
    seq_match(
        pat,
        path,
        |s| matches!(s, Seg::Globstar),
        |s, seg| match s {
            Seg::Pat(p) => seg_match(p, seg),
            Seg::Globstar => unreachable!("a globstar is the star element"),
        },
    )
}

/// Match one compiled segment against one path segment; a `*` consumes
/// any run of characters.
fn seg_match(pat: &[PatTok], text: &[char]) -> bool {
    seq_match(
        pat,
        text,
        |t| matches!(t, PatTok::Star),
        |t, &c| match t {
            PatTok::Tok(t) => tok_matches(t, c),
            PatTok::Star => unreachable!("a star is the star element"),
        },
    )
}

/// Whether `pat` matches all of `text`, where an `is_star` element
/// consumes any run of `text` (including none) and every other element
/// consumes exactly one item it `accepts`.
///
/// A row-by-row table over (pattern prefix, text prefix): `O(|pat| ×
/// |text|)` time, `O(|text|)` space, and no recursion. Backtracking
/// took exponential time on inputs like `*a*a*a*a*a*a*a*a*b` against a
/// long run of `a`s.
fn seq_match<P, T>(
    pat: &[P],
    text: &[T],
    is_star: impl Fn(&P) -> bool,
    accepts: impl Fn(&P, &T) -> bool,
) -> bool {
    // `row[j]`: the pattern prefix seen so far matches `text[..j]`.
    let mut row = vec![false; text.len() + 1];
    row[0] = true;
    for p in pat {
        if is_star(p) {
            // Reachable from any shorter prefix: a running OR.
            for j in 1..row.len() {
                row[j] = row[j] || row[j - 1];
            }
        } else {
            // Consume one item: shift right, filtering by `accepts`.
            for j in (1..row.len()).rev() {
                row[j] = row[j - 1] && accepts(p, &text[j - 1]);
            }
            row[0] = false;
        }
    }
    row[text.len()]
}

/// Whether one token matches one character.
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

/// Whether two globs could both match some path — the check behind
/// rejecting ambiguous rules.
fn glob_overlaps(a: &str, b: &str) -> bool {
    segs_intersect(&parse_glob(a), &parse_glob(b))
}

/// Whether two compiled segment lists have a common match.
fn segs_intersect(a: &[Seg], b: &[Seg]) -> bool {
    seq_intersect(
        a,
        b,
        |s| matches!(s, Seg::Globstar),
        |s| match s {
            Seg::Pat(p) => pat_nonempty(p),
            Seg::Globstar => true,
        },
        |x, y| match (x, y) {
            (Seg::Pat(x), Seg::Pat(y)) => pats_intersect(x, y),
            _ => unreachable!("globstars are star elements"),
        },
    )
}

/// Can two segment patterns match a common string?
fn pats_intersect(a: &[PatTok], b: &[PatTok]) -> bool {
    seq_intersect(
        a,
        b,
        |t| matches!(t, PatTok::Star),
        |t| match t {
            PatTok::Tok(t) => tok_nonempty(t),
            PatTok::Star => true,
        },
        |x, y| match (x, y) {
            (PatTok::Tok(x), PatTok::Tok(y)) => toks_intersect(x, y),
            _ => unreachable!("stars are star elements"),
        },
    )
}

/// Whether two element sequences can match a common string. An
/// `is_star` element matches any run (including none); every other
/// element matches one item, is satisfiable when `nonempty`, and
/// `pair` says whether two of them share an item.
///
/// The recurrence: with `x` and `y` the two remaining sequences, a
/// star at the head of `x` either matches nothing (drop it) or emits
/// one item that `y`'s head also covers (drop `y`'s head, keep the
/// star). A star only on `y` swaps the two sides. Two plain heads must
/// `pair`. So a state is `(i, j, swapped)` — the suffixes `a[i..]` and
/// `b[j..]`, in either order — and every state depends only on larger
/// `i`/`j` or on its own swapped twin, which never depends back.
/// Filling rows from the end is `O(|a| × |b|)` time and `O(|b|)`
/// space with no recursion; the backtracking it replaces was
/// exponential.
fn seq_intersect<T>(
    a: &[T],
    b: &[T],
    is_star: impl Fn(&T) -> bool,
    nonempty: impl Fn(&T) -> bool,
    pair: impl Fn(&T, &T) -> bool,
) -> bool {
    let (n, m) = (a.len(), b.len());
    // `*_next` hold row `i + 1`, `*_row` row `i`. `straight[j]` answers
    // `(a[i..], b[j..])`; `swapped[j]` answers `(b[j..], a[i..])`.
    let mut straight_next = vec![false; m + 1];
    let mut swapped_next = vec![false; m + 1];
    let mut straight_row = vec![false; m + 1];
    let mut swapped_row = vec![false; m + 1];
    for i in (0..=n).rev() {
        let a_star = i < n && is_star(&a[i]);
        // Whether `a[i]` can emit an item to match a star on the `b` side.
        let a_emits = i < n && (a_star || nonempty(&a[i]));
        for j in (0..=m).rev() {
            let b_star = j < m && is_star(&b[j]);
            let b_emits = j < m && (b_star || nonempty(&b[j]));
            if i == n && j == m {
                straight_row[j] = true;
                swapped_row[j] = true;
                continue;
            }
            // A star heading either side: it matches nothing, or emits
            // an item the other side's head also covers. Neither side
            // reads its twin here, so these go first.
            let star_straight = || straight_next[j] || (b_emits && straight_row[j + 1]);
            let star_swapped = || swapped_row[j + 1] || (a_emits && swapped_next[j]);
            let one_ends = i == n || j == m;
            let (straight, swapped) = match (a_star, b_star) {
                (true, true) => (star_straight(), star_swapped()),
                // The star sits on one side only; the other order swaps
                // onto it.
                (true, false) => {
                    let st = star_straight();
                    (st, st)
                }
                (false, true) => {
                    let sw = star_swapped();
                    (sw, sw)
                }
                (false, false) if one_ends => (false, false),
                (false, false) => (
                    pair(&a[i], &b[j]) && straight_next[j + 1],
                    pair(&b[j], &a[i]) && swapped_next[j + 1],
                ),
            };
            straight_row[j] = straight;
            swapped_row[j] = swapped;
        }
        std::mem::swap(&mut straight_next, &mut straight_row);
        std::mem::swap(&mut swapped_next, &mut swapped_row);
    }
    straight_next[0]
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

/// Whether two character-class items share any character.
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

    #[test]
    fn many_stars_do_not_backtrack_exponentially() {
        // Regression: backtracking never finished on these.
        let text = "a".repeat(95);
        assert!(!glob_match("*a*a*a*a*a*a*a*a*b", &text));
        assert!(glob_match("*a*a*a*a*a*a*a*a*a", &text));
        let path = vec!["a"; 60].join("/");
        assert!(!glob_match("**/a/**/a/**/a/**/a/**/a/**/a/**/b", &path));
        assert!(glob_match("**/a/**/a/**/a/**/a/**/a/**/a/**/a", &path));
        assert!(!glob_overlaps("*a*a*a*a*a*a*a*a*b", &text));
        assert!(!glob_overlaps(
            "*a*a*a*a*a*a*a*a*b",
            &format!("{}c", "*a".repeat(40))
        ));
        assert!(glob_overlaps(
            "*a*a*a*a*a*a*a*a*b",
            &format!("{}*", "*a".repeat(40))
        ));
        assert!(!glob_overlaps(
            "**/a/**/a/**/a/**/a/**/a/**/b",
            &format!("{path}/c")
        ));
    }

    #[test]
    fn long_patterns_do_not_recurse() {
        // Matching walks a table rather than recursing per character,
        // so a long pattern cannot exhaust the stack.
        let long = "a".repeat(5_000);
        assert!(glob_match(&long, &long));
        assert!(!glob_match(&format!("{long}b"), &long));
    }
}
