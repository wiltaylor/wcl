/// Unique identifier for a source file in the SourceMap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

/// A span of source text, expressed as byte offsets into a specific file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(file: FileId, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }

    /// A placeholder span that does not point to any real source location.
    pub fn dummy() -> Self {
        Span {
            file: FileId(0),
            start: 0,
            end: 0,
        }
    }

    /// Return the smallest span that covers both `self` and `other`.
    ///
    /// # Panics
    /// Panics if the two spans belong to different files.
    pub fn merge(self, other: Span) -> Span {
        assert_eq!(
            self.file, other.file,
            "cannot merge spans from different files"
        );
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Number of bytes covered by this span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A single source file stored in the [`SourceMap`].
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: String,
    pub source: String,
    /// Byte offset of the first character of each line (0-indexed lines).
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: FileId, path: String, source: String) -> Self {
        let line_starts = std::iter::once(0)
            .chain(
                source
                    .char_indices()
                    .filter_map(|(i, c)| if c == '\n' { Some(i + 1) } else { None }),
            )
            .collect();
        Self {
            id,
            path,
            source,
            line_starts,
        }
    }

    /// Return the 1-indexed `(line, column)` for a given byte offset.
    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        let line_idx = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let col = offset - self.line_starts[line_idx];
        ((line_idx + 1) as u32, (col + 1) as u32)
    }

    /// Return the source text slice covered by `span`.
    pub fn span_text(&self, span: Span) -> &str {
        &self.source[span.start..span.end]
    }
}

/// Registry of all source files in a compilation session.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file and return its assigned [`FileId`].
    pub fn add_file(&mut self, path: String, source: String) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile::new(id, path, source));
        id
    }

    pub fn get_file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn line_col(&self, file: FileId, offset: usize) -> (u32, u32) {
        self.get_file(file).line_col(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_file_returns_distinct_ids() {
        let mut sm = SourceMap::new();
        let a = sm.add_file("a.wcl".into(), "hello".into());
        let b = sm.add_file("b.wcl".into(), "world".into());
        assert_ne!(a, b);
        assert_eq!(sm.get_file(a).path, "a.wcl");
        assert_eq!(sm.get_file(b).path, "b.wcl");
    }

    #[test]
    fn line_col_single_line() {
        let mut sm = SourceMap::new();
        let id = sm.add_file("test.wcl".into(), "port = 8080".into());
        // The whole string is on line 1.
        assert_eq!(sm.line_col(id, 0), (1, 1));
        assert_eq!(sm.line_col(id, 7), (1, 8));
    }

    #[test]
    fn line_col_multiline() {
        let mut sm = SourceMap::new();
        let src = "foo\nbar\nbaz".to_string();
        let id = sm.add_file("test.wcl".into(), src);
        assert_eq!(sm.line_col(id, 0), (1, 1)); // 'f'
        assert_eq!(sm.line_col(id, 3), (1, 4)); // '\n' — still on line 1
        assert_eq!(sm.line_col(id, 4), (2, 1)); // 'b' of "bar"
        assert_eq!(sm.line_col(id, 8), (3, 1)); // 'b' of "baz"
    }

    #[test]
    fn span_merge() {
        let f = FileId(1);
        let a = Span::new(f, 2, 5);
        let b = Span::new(f, 8, 12);
        let m = a.merge(b);
        assert_eq!(m.start, 2);
        assert_eq!(m.end, 12);
    }

    #[test]
    fn span_text() {
        let mut sm = SourceMap::new();
        let id = sm.add_file("t.wcl".into(), "hello world".into());
        let span = Span::new(id, 6, 11);
        assert_eq!(sm.get_file(id).span_text(span), "world");
    }
}
