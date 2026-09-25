/*
 * External scanner for tree-sitter-wcl.
 *
 * Two WCL constructs depend on raw bytes rather than tokens, so the
 * generated lexer cannot see them:
 *
 * - Heredocs. `<<TAG`, `<<'TAG'`, `$<<TAG`, `ascii<<TAG`, `$utf16<<TAG`
 *   open a body that runs until a line holding only `TAG` (surrounding
 *   whitespace allowed). The whole literal, opener to closer, is one
 *   `heredoc` token.
 *
 * - The end of a body-less block. The Rust parser ends a block's label
 *   list at the first token preceded by a line break (a line comment
 *   counts as one), so `hr` on its own line is a complete block. This
 *   scanner emits the zero-width `_block_end` token in that position,
 *   and before a `}`, `@` or end of input on the same line.
 *
 * Mirrors `crates/wcl_lang/src/lexer/strings.rs` (heredocs) and
 * `Parser::parse_block` (label loop) — keep the three in step.
 */

#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <string.h>

enum TokenType {
  BLOCK_END,
  HEREDOC,
  ERROR_SENTINEL,
};

#define MAX_TAG 256

void *tree_sitter_wcl_external_scanner_create(void) { return NULL; }
void tree_sitter_wcl_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_wcl_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_wcl_external_scanner_deserialize(void *payload, const char *buffer,
                                                  unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static inline bool is_ident_start(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}

static inline bool is_ident_cont(int32_t c) {
  return is_ident_start(c) || (c >= '0' && c <= '9');
}

static inline bool is_inline_space(int32_t c) {
  return c == ' ' || c == '\t' || c == '\r';
}

/* A character that, first on the line after a block kind, continues the
 * current item instead of starting a new one: `=` (field), `->`
 * (connection), `.` / `::` (qualified kind), `:` (table). */
static inline bool continues_item(int32_t c) {
  return c == '=' || c == '-' || c == '.' || c == ':';
}

/* Consume the rest of a line comment, up to but not including `\n`. */
static void skip_to_line_end(TSLexer *lexer) {
  while (!lexer->eof(lexer) && lexer->lookahead != '\n') advance(lexer);
}

/* With the cursor on `#` or `/`, consume a line comment if there is one.
 * Returns false (having consumed at most the `/`) when it is not one. */
static bool consume_comment(TSLexer *lexer) {
  if (lexer->lookahead == '#') {
    skip_to_line_end(lexer);
    return true;
  }
  if (lexer->lookahead == '/') {
    advance(lexer);
    if (lexer->lookahead != '/') return false;
    skip_to_line_end(lexer);
    return true;
  }
  return false;
}

/* Decide whether a body-less block ends here. The token is zero-width:
 * `mark_end` has already been called, so anything consumed while looking
 * ahead is not part of it. */
static bool scan_block_end(TSLexer *lexer, bool saw_newline) {
  int32_t c = lexer->lookahead;
  if (lexer->eof(lexer) || c == '}') return true;
  if (c == '{') return false;

  if (c == '#' || c == '/') {
    /* A comment ends the line, so it ends the label list — unless the
     * body's `{` follows it, which the Rust parser still accepts. */
    if (!consume_comment(lexer)) return false;
    for (;;) {
      while (!lexer->eof(lexer) &&
             (is_inline_space(lexer->lookahead) || lexer->lookahead == '\n')) {
        advance(lexer);
      }
      if (lexer->lookahead == '#' || lexer->lookahead == '/') {
        if (!consume_comment(lexer)) return true;
        continue;
      }
      break;
    }
    if (lexer->eof(lexer)) return true;
    c = lexer->lookahead;
    return c != '{' && !continues_item(c);
  }

  if (saw_newline) return !continues_item(c);
  return c == '@' || c == ';';
}

static bool encoding_prefix(const char *word, unsigned len) {
  static const char *const names[] = {"ascii", "utf8", "utf16", "utf32"};
  for (unsigned i = 0; i < 4; i++) {
    if (strlen(names[i]) == len && strncmp(names[i], word, len) == 0) return true;
  }
  return false;
}

/* Scan a heredoc from its first character. Every path that returns false
 * leaves the literal unrecognised; tree-sitter then rewinds and lexes
 * the text normally. */
static bool scan_heredoc(TSLexer *lexer) {
  if (lexer->lookahead == '$') advance(lexer);

  if (is_ident_start(lexer->lookahead)) {
    char word[8];
    unsigned len = 0;
    while (is_ident_cont(lexer->lookahead)) {
      if (len >= sizeof word) return false;
      word[len++] = (char)lexer->lookahead;
      advance(lexer);
    }
    if (!encoding_prefix(word, len)) return false;
  }

  if (lexer->lookahead != '<') return false;
  advance(lexer);
  if (lexer->lookahead != '<') return false;
  advance(lexer);

  bool raw = false;
  if (lexer->lookahead == '\'') {
    raw = true;
    advance(lexer);
  }
  if (!is_ident_start(lexer->lookahead)) return false;

  char tag[MAX_TAG];
  unsigned tag_len = 0;
  while (is_ident_cont(lexer->lookahead)) {
    if (tag_len >= MAX_TAG) return false;
    tag[tag_len++] = (char)lexer->lookahead;
    advance(lexer);
  }
  if (raw) {
    if (lexer->lookahead != '\'') return false;
    advance(lexer);
  }

  /* Only spaces and a line comment may follow the tag on its line. */
  while (is_inline_space(lexer->lookahead)) advance(lexer);
  if (lexer->lookahead == '#' || lexer->lookahead == '/') {
    if (!consume_comment(lexer)) return false;
  }
  if (lexer->lookahead != '\n') return false;
  advance(lexer);

  /* Body lines, until one that is exactly the tag. */
  for (;;) {
    if (lexer->eof(lexer)) return false;
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
    unsigned matched = 0;
    while (matched < tag_len && lexer->lookahead == tag[matched]) {
      advance(lexer);
      matched++;
    }
    if (matched == tag_len) {
      lexer->mark_end(lexer);
      while (is_inline_space(lexer->lookahead)) advance(lexer);
      if (lexer->eof(lexer) || lexer->lookahead == '\n') return true;
    }
    skip_to_line_end(lexer);
    if (lexer->eof(lexer)) return false;
    advance(lexer); /* the `\n` */
  }
}

bool tree_sitter_wcl_external_scanner_scan(void *payload, TSLexer *lexer,
                                           const bool *valid_symbols) {
  (void)payload;
  /* Error recovery marks every symbol valid; stay out of its way. */
  if (valid_symbols[ERROR_SENTINEL]) return false;
  if (!valid_symbols[BLOCK_END] && !valid_symbols[HEREDOC]) return false;

  bool saw_newline = false;
  while (!lexer->eof(lexer) &&
         (is_inline_space(lexer->lookahead) || lexer->lookahead == '\n')) {
    if (lexer->lookahead == '\n') saw_newline = true;
    skip(lexer);
  }
  lexer->mark_end(lexer);

  if (valid_symbols[BLOCK_END]) {
    int32_t c = lexer->lookahead;
    /* A heredoc can be a label, so a same-line `<`, `$` or word is left
     * for the heredoc check (or the generated lexer) below. */
    bool maybe_label = !saw_newline && (c == '<' || c == '$' || is_ident_start(c));
    if (!maybe_label && scan_block_end(lexer, saw_newline)) {
      lexer->result_symbol = BLOCK_END;
      return true;
    }
    if (!maybe_label) return false;
  }

  if (valid_symbols[HEREDOC]) {
    int32_t c = lexer->lookahead;
    if (c == '<' || c == '$' || is_ident_start(c)) {
      if (scan_heredoc(lexer)) {
        lexer->result_symbol = HEREDOC;
        return true;
      }
    }
  }
  return false;
}
