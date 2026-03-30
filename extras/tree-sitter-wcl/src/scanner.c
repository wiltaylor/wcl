/**
 * Tree-sitter external scanner for WCL heredoc literals.
 *
 * Heredoc syntax:
 *   <<TAG       standard heredoc
 *   <<-TAG      indented heredoc (strips leading whitespace)
 *   <<'TAG'     raw heredoc (no interpolation/escapes)
 *   <<-'TAG'    indented + raw
 *
 * The closing TAG must appear on its own line (optional leading/trailing
 * whitespace is allowed).
 */

#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <string.h>

/* Token indices — must match the order in grammar.js `externals`. */
enum TokenType {
  HEREDOC_START,
  HEREDOC_BODY,
  HEREDOC_END,
};

#define MAX_DELIM 255

typedef struct {
  char delim[MAX_DELIM];
  uint8_t delim_len;
  bool active; /* true while inside a heredoc (between start and end) */
} Scanner;

/* ── Lifecycle ─────────────────────────────────────────────────────────── */

void *tree_sitter_wcl_external_scanner_create(void) {
  Scanner *s = calloc(1, sizeof(Scanner));
  return s;
}

void tree_sitter_wcl_external_scanner_destroy(void *payload) { free(payload); }

/* ── Serialization ─────────────────────────────────────────────────────── */

unsigned tree_sitter_wcl_external_scanner_serialize(void *payload,
                                                    char *buffer) {
  Scanner *s = (Scanner *)payload;
  if (!s->active) {
    buffer[0] = 0;
    return 1;
  }
  buffer[0] = 1;
  buffer[1] = (char)s->delim_len;
  memcpy(buffer + 2, s->delim, s->delim_len);
  return 2 + s->delim_len;
}

void tree_sitter_wcl_external_scanner_deserialize(void *payload,
                                                   const char *buffer,
                                                   unsigned length) {
  Scanner *s = (Scanner *)payload;
  s->active = false;
  s->delim_len = 0;
  if (length == 0) return;
  if (buffer[0] == 0) return;
  if (length < 2) return;
  s->active = true;
  s->delim_len = (uint8_t)buffer[1];
  if (length < 2u + s->delim_len) {
    s->active = false;
    return;
  }
  memcpy(s->delim, buffer + 2, s->delim_len);
}

/* ── Helpers ───────────────────────────────────────────────────────────── */

static inline bool is_ident_start(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}

static inline bool is_ident_char(int32_t c) {
  return is_ident_start(c) || (c >= '0' && c <= '9');
}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static void skip_ws(TSLexer *lexer) { lexer->advance(lexer, true); }


/* ── Scan ──────────────────────────────────────────────────────────────── */

bool tree_sitter_wcl_external_scanner_scan(void *payload, TSLexer *lexer,
                                           const bool *valid_symbols) {
  Scanner *s = (Scanner *)payload;

  /* ── Mode A: look for heredoc_start ─────────────────────────────── */
  if (!s->active && valid_symbols[HEREDOC_START]) {
    /* Skip whitespace that tree-sitter would normally skip (extras). */
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
      skip_ws(lexer);
    }

    if (lexer->lookahead != '<') return false;
    advance(lexer);
    if (lexer->lookahead != '<') return false;
    advance(lexer);

    /* Optional '-' (indented). */
    if (lexer->lookahead == '-') advance(lexer);

    /* Optional opening quote (raw). */
    bool raw = false;
    if (lexer->lookahead == '\'') {
      raw = true;
      advance(lexer);
    }

    /* Read the delimiter identifier. */
    if (!is_ident_start(lexer->lookahead)) return false;
    s->delim_len = 0;
    while (is_ident_char(lexer->lookahead) && s->delim_len < MAX_DELIM) {
      s->delim[s->delim_len++] = (char)lexer->lookahead;
      advance(lexer);
    }

    /* Closing quote for raw. */
    if (raw) {
      if (lexer->lookahead != '\'') return false;
      advance(lexer);
    }

    /* Consume to end of line (including the newline). */
    while (lexer->lookahead != '\n' && lexer->lookahead != '\r' &&
           !lexer->eof(lexer)) {
      advance(lexer);
    }
    if (lexer->lookahead == '\r') advance(lexer);
    if (lexer->lookahead == '\n') advance(lexer);

    s->active = true;
    lexer->result_symbol = HEREDOC_START;
    return true;
  }

  /* ── Mode B: inside a heredoc — look for body or end ────────────── */
  if (s->active && (valid_symbols[HEREDOC_BODY] || valid_symbols[HEREDOC_END])) {
    lexer->mark_end(lexer);

    /* Try to match HEREDOC_END first. If the current line is the closing
       delimiter, emit it. Otherwise fall through to HEREDOC_BODY. */
    if (valid_symbols[HEREDOC_END]) {
      bool at_delim = false;

      int32_t ch = lexer->lookahead;

      /* Quick check: is this potentially a delimiter line? */
      /* Skip whitespace first. */
      while (ch == ' ' || ch == '\t') {
        advance(lexer);
        ch = lexer->lookahead;
      }

      /* Now check if next chars match the delimiter. */
      if (is_ident_start(ch)) {
        bool matches = true;
        for (uint8_t i = 0; i < s->delim_len; i++) {
          if (lexer->lookahead != (int32_t)s->delim[i]) {
            matches = false;
            break;
          }
          advance(lexer);
        }

        if (matches) {
          /* Check that only whitespace follows until newline/EOF. */
          while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
            advance(lexer);
          }
          if (lexer->lookahead == '\n') {
            advance(lexer);
            at_delim = true;
          } else if (lexer->lookahead == '\r') {
            advance(lexer);
            if (lexer->lookahead == '\n') advance(lexer);
            at_delim = true;
          } else if (lexer->eof(lexer)) {
            at_delim = true;
          }
        }
      } else if (ch == '\n' || ch == '\r' || lexer->eof(lexer)) {
        /* Empty line — not a delimiter. */
        at_delim = false;
      }

      if (at_delim) {
        lexer->mark_end(lexer);
        lexer->result_symbol = HEREDOC_END;
        s->active = false;
        s->delim_len = 0;
        return true;
      }
    }

    /* Not the delimiter line. Consume lines as HEREDOC_BODY. */
    if (!valid_symbols[HEREDOC_BODY]) return false;

    /* We've already consumed some chars from the failed delimiter check.
       We need to finish this line, then keep consuming until we hit the
       delimiter or EOF. */

    /* Finish current line. */
    while (lexer->lookahead != '\n' && lexer->lookahead != '\r' &&
           !lexer->eof(lexer)) {
      advance(lexer);
    }
    if (lexer->lookahead == '\r') advance(lexer);
    if (lexer->lookahead == '\n') advance(lexer);
    lexer->mark_end(lexer);

    /* Continue consuming lines that are NOT the closing delimiter. */
    while (!lexer->eof(lexer)) {
      /* Peek at the current line to see if it's the delimiter. */
      /* Skip leading whitespace. */
      int32_t ch = lexer->lookahead;
      while (ch == ' ' || ch == '\t') {
        advance(lexer);
        ch = lexer->lookahead;
      }

      /* Check for delimiter match. */
      if (is_ident_start(ch)) {
        bool matches = true;
        for (uint8_t i = 0; i < s->delim_len; i++) {
          if (lexer->lookahead != (int32_t)s->delim[i]) {
            matches = false;
            break;
          }
          advance(lexer);
        }

        if (matches) {
          /* Check trailing content. */
          int32_t after = lexer->lookahead;
          while (after == ' ' || after == '\t') {
            advance(lexer);
            after = lexer->lookahead;
          }
          if (after == '\n' || after == '\r' || lexer->eof(lexer)) {
            /* This IS the delimiter line. Don't include it in body. */
            lexer->result_symbol = HEREDOC_BODY;
            return true;
          }
        }
      }

      /* Not a delimiter line — consume to end of line. */
      while (lexer->lookahead != '\n' && lexer->lookahead != '\r' &&
             !lexer->eof(lexer)) {
        advance(lexer);
      }
      if (lexer->lookahead == '\r') advance(lexer);
      if (lexer->lookahead == '\n') advance(lexer);
      lexer->mark_end(lexer);
    }

    /* Reached EOF without finding the closing delimiter.
       Emit whatever we consumed as body. */
    lexer->result_symbol = HEREDOC_BODY;
    return true;
  }

  return false;
}
