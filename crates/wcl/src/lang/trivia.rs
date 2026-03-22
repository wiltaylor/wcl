use crate::lang::span::Span;

/// All trivia (comments + blank-line count) attached to an AST node.
#[derive(Debug, Clone, PartialEq)]
pub struct Trivia {
    pub comments: Vec<Comment>,
    /// Number of blank lines immediately preceding this node.
    /// Preserved so formatters can reproduce the author's grouping.
    pub leading_newlines: u32,
}

impl Trivia {
    pub fn empty() -> Self {
        Self {
            comments: Vec::new(),
            leading_newlines: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.comments.is_empty() && self.leading_newlines == 0
    }
}

impl Default for Trivia {
    fn default() -> Self {
        Self::empty()
    }
}

/// A single comment token together with its metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    /// The raw text of the comment, including delimiters (e.g. `// foo` or `/* bar */`).
    pub text: String,
    pub style: CommentStyle,
    pub placement: CommentPlacement,
    pub span: Span,
}

/// Which syntactic form a comment uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// `// …` — single-line comment.
    Line,
    /// `/* … */` — block comment (may nest).
    Block,
    /// `/// …` — doc comment; always attaches to the next declaration.
    Doc,
}

/// Where a comment sits relative to the AST node it is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentPlacement {
    /// On the line(s) immediately before the node, with no intervening blank line.
    Leading,
    /// On the same line, after the node's value.
    Trailing,
    /// Inside a block body, not adjacent to any attribute or child block.
    Floating,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::{FileId, Span};

    #[test]
    fn trivia_empty_is_default() {
        let t: Trivia = Default::default();
        assert!(t.is_empty());
        assert_eq!(t, Trivia::empty());
    }

    #[test]
    fn trivia_with_comment_is_not_empty() {
        let span = Span::dummy();
        let c = Comment {
            text: "// hello".into(),
            style: CommentStyle::Line,
            placement: CommentPlacement::Leading,
            span,
        };
        let t = Trivia {
            comments: vec![c],
            leading_newlines: 0,
        };
        assert!(!t.is_empty());
    }

    #[test]
    fn trivia_with_newlines_is_not_empty() {
        let t = Trivia {
            comments: vec![],
            leading_newlines: 1,
        };
        assert!(!t.is_empty());
    }

    #[test]
    fn comment_styles_are_distinct() {
        assert_ne!(CommentStyle::Line, CommentStyle::Block);
        assert_ne!(CommentStyle::Block, CommentStyle::Doc);
        assert_ne!(CommentStyle::Line, CommentStyle::Doc);
    }

    #[test]
    fn comment_placements_are_distinct() {
        assert_ne!(CommentPlacement::Leading, CommentPlacement::Trailing);
        assert_ne!(CommentPlacement::Leading, CommentPlacement::Floating);
        assert_ne!(CommentPlacement::Trailing, CommentPlacement::Floating);
    }

    #[test]
    fn comment_span_round_trip() {
        let fid = FileId(42);
        let span = Span::new(fid, 10, 20);
        let c = Comment {
            text: "/* test */".into(),
            style: CommentStyle::Block,
            placement: CommentPlacement::Trailing,
            span,
        };
        assert_eq!(c.span.file, fid);
        assert_eq!(c.span.start, 10);
        assert_eq!(c.span.end, 20);
    }
}
