//! Span ↔ LSP position conversion.
//!
//! `wcl_lang::Span` is a half-open byte range. LSP positions are
//! `(line, character)` pairs, where `character` counts code units of the
//! negotiated [`PositionEncoding`]: UTF-8 bytes when the client offers
//! them, otherwise UTF-16 code units (the protocol's mandatory default,
//! and the only encoding VS Code speaks). Lines break at `\n` only.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use ropey::Rope;
use tower_lsp_server::ls_types::{ClientCapabilities, Position, PositionEncodingKind, Range, Uri};
use wcl_lang::Span;

/// The filesystem path a `file:` URI names, in its
/// [`path_key`](wcl_lang::path_key) spelling. `None` for any other
/// scheme (`untitled:`, `vscode-notebook-cell:` …), which has no path
/// on disk to import from or read back.
///
/// A URI with a host other than `localhost` names a file on that host:
/// on Windows `file://server/share/a.wcl` is the share path
/// `\\server\share\a.wcl`; elsewhere there is no path for it, so
/// `None`. `file://localhost/C:/x` is the local `C:\x`.
pub(crate) fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file") {
        return None;
    }
    let host = uri.authority().map(|a| a.host()).unwrap_or_default();
    let path = uri.path().decode().to_string_lossy();
    let text = file_path_text(&percent_decode(host), &path, cfg!(windows))?;
    Some(wcl_lang::path_key(Path::new(&text)))
}

/// The path text a `file:` URI's decoded `host` and `path` name, read
/// the Windows way when `windows` is set. See [`uri_to_path`].
fn file_path_text(host: &str, path: &str, windows: bool) -> Option<String> {
    let remote = !host.is_empty() && !host.eq_ignore_ascii_case("localhost");
    if path.is_empty() && !remote {
        return None;
    }
    if !windows {
        return (!remote).then(|| path.to_string());
    }
    if remote {
        // A share path needs a share as well as the host.
        let share = path.trim_start_matches('/');
        if share.is_empty() {
            return None;
        }
        return Some(format!(r"\\{host}\{}", share.replace('/', "\\")));
    }
    // `/C:/x` is the drive path `C:\x`; the leading slash only separates
    // it from the (empty) host.
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && bytes.get(3).is_none_or(|&b| b == b'/');
    let path = if drive { &path[1..] } else { path };
    Some(path.replace('/', "\\"))
}

/// The `file:` URI for an absolute path. `None` when the path is
/// relative. A Windows verbatim path (`\\?\C:\...`) is sent in its
/// plain form, since no editor opens `file:///%3F/C%3A/...`, and a
/// share path (`\\server\share\a.wcl`, or its verbatim
/// `\\?\UNC\server\...`) as `file://server/share/a.wcl`, the spelling
/// editors use.
///
/// An open file is reported under the URI the client sent, not this
/// one: see [`Ctx::uri_for`](crate::ctx::Ctx::uri_for).
pub(crate) fn path_to_uri(path: &Path) -> Option<Uri> {
    let path = wcl_lang::path_key(path);
    if !path.is_absolute() {
        return None;
    }
    if let Some(uri) = path.to_str().and_then(unc_uri) {
        return Uri::from_str(&uri).ok();
    }
    Uri::from_file_path(&path)
}

/// The `file://host/share/...` URI text for the Windows share path
/// `\\host\share\...`. `None` for any other path, including the
/// `\\?\` and `\\.\` device namespaces.
fn unc_uri(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\")?;
    if rest.starts_with(r"?\") || rest.starts_with(r".\") {
        return None;
    }
    let (host, share) = rest.split_once('\\')?;
    if host.is_empty() || share.is_empty() {
        return None;
    }
    Some(format!(
        "file://{}/{}",
        percent_encode(host),
        percent_encode(&share.replace('\\', "/"))
    ))
}

/// `text` with every byte outside RFC 3986's unreserved set and `/`
/// percent-encoded, as `Uri::from_file_path` encodes a path.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `text` with its `%XX` escapes decoded; an invalid escape or byte
/// sequence is kept (lossily) rather than rejected.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).and_then(|&b| hex(b)),
                bytes.get(i + 2).and_then(|&b| hex(b)),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The unit an LSP `character` counts. Negotiated once, in
/// `initialize`, and applied to every position crossing the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PositionEncoding {
    /// Bytes of UTF-8 — offered by some clients, native to `wcl_lang`.
    Utf8,
    /// UTF-16 code units — what every client must support.
    #[default]
    Utf16,
}

impl PositionEncoding {
    /// Pick UTF-8 when the client lists it in
    /// `general.positionEncodings`, otherwise fall back to UTF-16.
    pub(crate) fn negotiate(capabilities: &ClientCapabilities) -> Self {
        let offers_utf8 = capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .is_some_and(|offered| offered.contains(&PositionEncodingKind::UTF8));
        if offers_utf8 { Self::Utf8 } else { Self::Utf16 }
    }

    /// The protocol name advertised back in the server capabilities.
    pub(crate) fn kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
        }
    }

    /// Code units `text` occupies in this encoding.
    pub(crate) fn units(self, text: &str) -> usize {
        match self {
            Self::Utf8 => text.len(),
            Self::Utf16 => text.encode_utf16().count(),
        }
    }
}

/// Line starts of one text snapshot, so each conversion costs a binary
/// search plus a walk of a single line rather than a scan from the top
/// of the file. Build one per snapshot and reuse it for every span.
pub(crate) struct LineIndex<'a> {
    /// The snapshot being indexed.
    text: &'a str,
    /// Byte offset of the first byte of every line.
    line_starts: Vec<usize>,
    /// Unit `character` counts in.
    encoding: PositionEncoding,
}

impl<'a> LineIndex<'a> {
    /// Index `text` for conversions in `encoding`.
    pub(crate) fn new(text: &'a str, encoding: PositionEncoding) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self {
            text,
            line_starts,
            encoding,
        }
    }

    /// The encoding `character` counts in.
    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.encoding
    }

    /// Byte offset → LSP position. Offsets past the end clamp to the end
    /// of the text; an offset inside a multi-byte character snaps back to
    /// that character's first byte.
    pub(crate) fn position(&self, offset: usize) -> Position {
        let offset = self.text.floor_char_boundary(offset);
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let start = self.line_starts[line];
        Position {
            line: line as u32,
            character: self.encoding.units(&self.text[start..offset]) as u32,
        }
    }

    /// LSP position → byte offset. A line past the end clamps to the end
    /// of the text, a character past the end of its line clamps to the
    /// line's `\n`, and a character inside a multi-unit character (the
    /// middle of a UTF-8 sequence or a surrogate pair) snaps back to that
    /// character's first byte — so the result is always a char boundary.
    pub(crate) fn offset(&self, pos: Position) -> usize {
        let Some(&start) = self.line_starts.get(pos.line as usize) else {
            return self.text.len();
        };
        let end = self
            .line_starts
            .get(pos.line as usize + 1)
            .map_or(self.text.len(), |next| next - 1);
        let line = &self.text[start..end];
        let wanted = pos.character as usize;
        match self.encoding {
            PositionEncoding::Utf8 => start + line.floor_char_boundary(wanted),
            PositionEncoding::Utf16 => {
                let mut units = 0;
                for (i, c) in line.char_indices() {
                    units += c.len_utf16();
                    if units > wanted {
                        return start + i;
                    }
                }
                end
            }
        }
    }

    /// Byte [`Span`] → LSP [`Range`].
    pub(crate) fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Range covering the whole text — full-document formatting edits.
    pub(crate) fn full_range(&self) -> Range {
        Range {
            start: Position::new(0, 0),
            end: self.position(self.text.len()),
        }
    }
}

/// LSP position → char index in a rope, with the same clamping and
/// snapping as [`LineIndex::offset`]. Applies incremental edits without
/// materialising the buffer as a string per change event.
pub(crate) fn rope_char_index(rope: &Rope, pos: Position, encoding: PositionEncoding) -> usize {
    let line = pos.line as usize;
    if line >= rope.len_lines() {
        return rope.len_chars();
    }
    let start = rope.line_to_char(line);
    let end = if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1) - 1
    } else {
        rope.len_chars()
    };
    let wanted = pos.character as usize;
    match encoding {
        PositionEncoding::Utf8 => {
            let end_byte = rope.char_to_byte(end);
            rope.byte_to_char((rope.char_to_byte(start) + wanted).min(end_byte))
        }
        PositionEncoding::Utf16 => {
            let end_unit = rope.char_to_utf16_cu(end);
            rope.utf16_cu_to_char((rope.char_to_utf16_cu(start) + wanted).min(end_unit))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;

    fn utf8(text: &str) -> LineIndex<'_> {
        LineIndex::new(text, PositionEncoding::Utf8)
    }

    fn utf16(text: &str) -> LineIndex<'_> {
        LineIndex::new(text, PositionEncoding::Utf16)
    }

    #[test]
    fn offset_zero_is_origin() {
        assert_eq!(utf8("hello").position(0), Position::new(0, 0));
    }

    #[test]
    fn offset_past_newlines_advances_line() {
        assert_eq!(utf8("ab\ncd\nef").position(6), Position::new(2, 0));
    }

    #[test]
    fn offset_clamped_to_end() {
        assert_eq!(utf8("abc").position(999), Position::new(0, 3));
    }

    #[test]
    fn position_round_trips_with_offset() {
        let src = "ab\ncdef\nghi";
        for index in [utf8(src), utf16(src)] {
            for offset in 0..=src.len() {
                let round = index.offset(index.position(offset));
                assert_eq!(round, offset, "offset {offset} round-tripped to {round}");
            }
        }
    }

    #[test]
    fn position_past_line_end_clamps() {
        assert_eq!(utf8("ab\ncdef").offset(Position::new(0, 99)), 2);
        assert_eq!(utf16("ab\ncdef").offset(Position::new(0, 99)), 2);
    }

    #[test]
    fn position_past_document_clamps() {
        assert_eq!(utf8("abc").offset(Position::new(99, 0)), 3);
    }

    #[test]
    fn span_to_range_spans_two_lines() {
        let r = utf8("ab\ncdef").range(Span::new(1, 5));
        assert_eq!(r.start, Position::new(0, 1));
        assert_eq!(r.end, Position::new(1, 2));
    }

    #[test]
    fn non_ascii_columns_count_in_the_negotiated_unit() {
        // `é` is 2 bytes / 1 unit, `😀` 4 bytes / 2 units, `—` 3 bytes / 1 unit.
        let src = "x = \"é😀—\" y\n";
        let y = src.find('y').unwrap();
        assert_eq!(utf8(src).position(y), Position::new(0, 16));
        assert_eq!(utf16(src).position(y), Position::new(0, 11));
        assert_eq!(utf8(src).offset(Position::new(0, 16)), y);
        assert_eq!(utf16(src).offset(Position::new(0, 11)), y);
        for index in [utf8(src), utf16(src)] {
            for (offset, _) in src.char_indices() {
                assert_eq!(index.offset(index.position(offset)), offset);
            }
        }
    }

    #[test]
    fn mid_character_positions_snap_to_the_character_start() {
        let src = "é😀";
        // Byte 1 is inside `é`; UTF-16 unit 2 is inside the surrogate pair.
        assert_eq!(utf8(src).offset(Position::new(0, 1)), 0);
        assert_eq!(utf16(src).offset(Position::new(0, 2)), 2);
        assert_eq!(utf8(src).position(1), Position::new(0, 0));
        assert_eq!(utf16(src).position(3), Position::new(0, 1));
    }

    #[test]
    fn rope_index_agrees_with_line_index() {
        let src = "a é\n😀 — b\n\nlast";
        let rope = Rope::from_str(src);
        for encoding in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            let index = LineIndex::new(src, encoding);
            for line in 0..6 {
                for character in 0..12 {
                    let pos = Position::new(line, character);
                    let char_index = rope_char_index(&rope, pos, encoding);
                    assert_eq!(
                        rope.char_to_byte(char_index),
                        index.offset(pos),
                        "{encoding:?} {pos:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn negotiation_prefers_utf8_only_when_offered() {
        use tower_lsp_server::ls_types::GeneralClientCapabilities;
        let offering = |kinds: Vec<PositionEncodingKind>| ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: Some(kinds),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            PositionEncoding::negotiate(&ClientCapabilities::default()),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(&offering(vec![PositionEncodingKind::UTF16])),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(&offering(vec![
                PositionEncodingKind::UTF16,
                PositionEncodingKind::UTF8,
            ])),
            PositionEncoding::Utf8
        );
    }

    #[test]
    fn a_uri_host_names_a_share_on_windows_only() {
        let windows = |host, path| file_path_text(host, path, true);
        let unix = |host, path| file_path_text(host, path, false);
        assert_eq!(
            windows("server", "/share/dir/a.wcl").as_deref(),
            Some(r"\\server\share\dir\a.wcl")
        );
        assert_eq!(windows("server", "/"), None);
        assert_eq!(windows("server", ""), None);
        assert_eq!(unix("server", "/share/dir/a.wcl"), None);
        // `localhost` and the empty host are this machine.
        assert_eq!(
            windows("localhost", "/C:/x/a.wcl").as_deref(),
            Some(r"C:\x\a.wcl")
        );
        assert_eq!(windows("LocalHost", "/c:/x").as_deref(), Some(r"c:\x"));
        assert_eq!(windows("", "/c:/x/a.wcl").as_deref(), Some(r"c:\x\a.wcl"));
        assert_eq!(windows("", "/c:").as_deref(), Some("c:"));
        assert_eq!(
            unix("localhost", "/tmp/a.wcl").as_deref(),
            Some("/tmp/a.wcl")
        );
        assert_eq!(unix("", "/c:/x").as_deref(), Some("/c:/x"));
        // No drive: the slash is part of the path, not a separator.
        assert_eq!(windows("", "/a.wcl").as_deref(), Some(r"\a.wcl"));
        assert_eq!(windows("", "/cd:/x").as_deref(), Some(r"\cd:\x"));
        assert_eq!(windows("", ""), None);
    }

    #[test]
    fn a_share_path_takes_the_uri_spelling_editors_use() {
        assert_eq!(
            unc_uri(r"\\server\share\dir\a b.wcl").as_deref(),
            Some("file://server/share/dir/a%20b.wcl")
        );
        assert_eq!(
            unc_uri(r"\\server\share").as_deref(),
            Some("file://server/share")
        );
        for not_a_share in [
            r"\\?\UNC\h\s\a",
            r"\\.\pipe\x",
            r"\\server",
            r"C:\x",
            "/tmp/x",
        ] {
            assert_eq!(unc_uri(not_a_share), None, "{not_a_share}");
        }
        let uri = Uri::from_str(&unc_uri(r"\\server\share\a.wcl").unwrap()).unwrap();
        let host = uri.authority().map(|a| a.host());
        assert_eq!(host, Some("server"));
        assert_eq!(uri.path().as_str(), "/share/a.wcl");
    }

    #[test]
    fn percent_escapes_decode_and_encode() {
        assert_eq!(percent_decode("my%2Dhost%zz%4"), "my-host%zz%4");
        assert_eq!(percent_encode("a b/c:d-é"), "a%20b/c%3Ad-%C3%A9");
    }

    #[test]
    fn a_uri_naming_another_host_is_no_local_path_off_windows() {
        let uri = Uri::from_str("file://server/share/a.wcl").unwrap();
        #[cfg(windows)]
        assert_eq!(
            uri_to_path(&uri),
            Some(PathBuf::from(r"\\server\share\a.wcl"))
        );
        #[cfg(not(windows))]
        assert_eq!(uri_to_path(&uri), None);
    }

    #[test]
    fn a_relative_path_has_no_uri() {
        assert_eq!(path_to_uri(Path::new("a.wcl")), None);
    }

    #[test]
    fn an_open_file_is_reported_under_the_uri_the_client_sent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my-ws.wcl");
        std::fs::write(&path, "x = 1\n").unwrap();
        let built = path_to_uri(&path).unwrap();
        let sent = Uri::from_str(&built.as_str().replace("my-ws", "my%2Dws")).unwrap();
        assert_ne!(sent, built);
        let key = uri_to_path(&sent).unwrap();
        let ctx = crate::ctx::Ctx::with_buffers(
            PositionEncoding::Utf16,
            HashMap::from([(key.clone(), "x = 1\n".to_string())]),
            crate::host::wdoc(),
        )
        .with_client_uris(Arc::new(HashMap::from([(key, sent.clone())])));
        assert_eq!(ctx.uri_for(&path), Some(sent.clone()));
        let canonical = crate::ctx::canonical(&path);
        assert_eq!(ctx.uri_for(&canonical), Some(sent));
    }

    #[cfg(windows)]
    #[test]
    fn both_drive_spellings_open_the_same_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::ctx::canonical(&dir.path().join("main.wcl"));
        std::fs::write(&path, "on disk").unwrap();
        let built = path_to_uri(&path).unwrap();
        // VS Code's spelling: lowercase drive, `:` escaped.
        let drive = path.to_str().unwrap()[..1].to_ascii_lowercase();
        let rest = &built.as_str()["file:///C%3A".len()..];
        let vscode = Uri::from_str(&format!("file:///{drive}%3A{rest}")).unwrap();
        let plain = Uri::from_str(&format!("file:///{}:{rest}", drive.to_uppercase())).unwrap();
        assert_eq!(uri_to_path(&vscode), uri_to_path(&plain));
        assert_eq!(uri_to_path(&vscode).as_deref(), Some(path.as_path()));

        let key = uri_to_path(&vscode).unwrap();
        let ctx = crate::ctx::Ctx::with_buffers(
            PositionEncoding::Utf16,
            HashMap::from([(key.clone(), "unsaved".to_string())]),
            crate::host::wdoc(),
        )
        .with_client_uris(Arc::new(HashMap::from([(key, vscode.clone())])));
        assert_eq!(
            ctx.text(&uri_to_path(&plain).unwrap()).as_deref(),
            Some("unsaved")
        );
        assert_eq!(ctx.uri_for(&path), Some(vscode.clone()));
        assert_eq!(ctx.uri_for(&uri_to_path(&plain).unwrap()), Some(vscode));
    }

    #[cfg(windows)]
    #[test]
    fn a_localhost_uri_is_a_local_path() {
        let uri = Uri::from_str("file://localhost/C:/Users/me/a.wcl").unwrap();
        assert_eq!(uri_to_path(&uri), Some(PathBuf::from(r"C:\Users\me\a.wcl")));
        let uri = Uri::from_str("file://localhost/c%3A/Users/me/a.wcl").unwrap();
        assert_eq!(uri_to_path(&uri), Some(PathBuf::from(r"C:\Users\me\a.wcl")));
    }

    #[cfg(windows)]
    #[test]
    fn paths_and_uris_round_trip() {
        for path in [r"C:\Users\me\a b.wcl", r"\\server\share\dir\a.wcl"] {
            let uri = path_to_uri(Path::new(path)).unwrap();
            assert_eq!(
                uri_to_path(&uri),
                Some(PathBuf::from(path)),
                "{}",
                uri.as_str()
            );
        }
        let share = path_to_uri(Path::new(r"\\?\UNC\server\share\a.wcl")).unwrap();
        assert_eq!(share.as_str(), "file://server/share/a.wcl");
        for uri in ["file://server/share/a.wcl", "file:///C%3A/Users/me/a.wcl"] {
            let parsed = Uri::from_str(uri).unwrap();
            let path = uri_to_path(&parsed).unwrap();
            assert_eq!(
                path_to_uri(&path).unwrap().as_str(),
                uri,
                "{}",
                path.display()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_verbatim_path_is_sent_in_its_plain_spelling() {
        let verbatim = path_to_uri(Path::new(r"\\?\C:\Users\me\main.wcl")).unwrap();
        let plain = path_to_uri(Path::new(r"C:\Users\me\main.wcl")).unwrap();
        assert_eq!(verbatim, plain);
        assert!(!verbatim.as_str().contains("%3F"), "{}", verbatim.as_str());
    }

    #[cfg(windows)]
    #[test]
    fn a_canonical_path_has_no_verbatim_prefix() {
        let dir = std::env::temp_dir();
        let canonical = crate::ctx::canonical(&dir);
        assert!(
            !canonical.as_os_str().to_string_lossy().starts_with(r"\\?\"),
            "{}",
            canonical.display()
        );
        let uri = path_to_uri(&canonical).unwrap();
        assert!(!uri.as_str().contains("%3F"), "{}", uri.as_str());
    }
}
