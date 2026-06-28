//! Number-literal lexing. Extracted from `lexer.rs` so the parent file
//! can stay focused on the dispatch state machine.

use crate::ast::Span;
use crate::numeric::{self, ParsedNumber};

use super::{LexError, Lexer, Token, TokenKind};

impl<'a> Lexer<'a> {
    pub(super) fn lex_number(&mut self, start: usize) -> Result<Token, LexError> {
        let neg = if self.peek() == Some(b'-') {
            self.pos += 1;
            true
        } else {
            false
        };

        // Detect base prefix (only after optional sign, at start of digits).
        let base = match (self.peek(), self.peek_at(1)) {
            (Some(b'0'), Some(b'x' | b'X')) => {
                self.pos += 2;
                16
            }
            (Some(b'0'), Some(b'b' | b'B')) => {
                self.pos += 2;
                2
            }
            (Some(b'0'), Some(b'o' | b'O')) => {
                self.pos += 2;
                8
            }
            _ => 10,
        };

        let body_start = self.pos;
        let body_scan = self.scan_digits_with_underscores(|c| is_digit_in_base(c, base))?;
        if !body_scan.had_digit {
            // If the next character is alphanumeric, treat it as a bad digit
            // for the chosen base (more helpful than "expected digits").
            if let Some(c) = self.peek()
                && c.is_ascii_alphanumeric()
            {
                return Err(LexError {
                    message: format!("invalid digit '{}' for base {base}", c as char),
                    span: Span::new(self.pos, self.pos + 1),
                });
            }
            return Err(LexError {
                message: "expected digits in numeric literal".into(),
                span: Span::new(start, self.pos),
            });
        }
        if body_scan.trailing_underscore {
            return Err(LexError {
                message: "trailing underscore in numeric literal".into(),
                span: Span::new(self.pos - 1, self.pos),
            });
        }
        let body_end = self.pos;

        let mut is_float = false;
        let mut frac_end = body_end;
        if base == 10 && self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            is_float = true;
            self.pos += 1; // consume '.'
            let frac_scan = self.scan_digits_with_underscores(|c| c.is_ascii_digit())?;
            if frac_scan.trailing_underscore {
                return Err(LexError {
                    message: "trailing underscore in numeric literal".into(),
                    span: Span::new(self.pos - 1, self.pos),
                });
            }
            frac_end = self.pos;
        }

        let mut exponent_text: Option<String> = None;
        if base == 10 && is_float && matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1; // consume 'e'
            let exp_start = self.pos;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_scan = self.scan_digits_with_underscores(|c| c.is_ascii_digit())?;
            if !exp_scan.had_digit {
                return Err(LexError {
                    message: "expected digits in exponent".into(),
                    span: Span::new(exp_start, self.pos),
                });
            }
            exponent_text = Some(
                std::str::from_utf8(&self.src[exp_start..self.pos])
                    .expect("ASCII exponent")
                    .replace('_', ""),
            );
        }

        let suffix_start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let suffix =
            std::str::from_utf8(&self.src[suffix_start..self.pos]).expect("suffix is ASCII");
        let literal_span = Span::new(start, self.pos);

        // Build the cleaned body string for the numeric helper.
        let body_text = std::str::from_utf8(&self.src[body_start..body_end]).expect("ASCII digits");
        let body_clean = body_text.replace('_', "");
        let body_for_finalize: String = if is_float {
            let frac_text =
                std::str::from_utf8(&self.src[body_end..frac_end]).expect("ASCII digits");
            let frac_clean = frac_text.replace('_', "");
            format!("{body_clean}{frac_clean}")
        } else {
            body_clean.clone()
        };

        let parsed = ParsedNumber {
            neg,
            base,
            body: &body_for_finalize,
            exponent: exponent_text.as_deref(),
            is_float,
            suffix,
        };

        numeric::finalize(parsed)
            .map(|fin| {
                let kind = match fin.unit {
                    Some(unit) => TokenKind::NumberWithUnit(Box::new((fin.lit, unit))),
                    None => TokenKind::Number(fin.lit),
                };
                Token::new(kind, literal_span)
            })
            .map_err(|e| LexError {
                message: e.message,
                span: literal_span,
            })
    }

    /// Advance through a digit run that allows `_` as a thousands separator.
    /// Underscores may only follow a digit; a leading `_` (or `_` after `_`)
    /// is a hard error. Reports whether at least one digit was consumed and
    /// whether the run ended on `_`. Callers decide what to do with both.
    fn scan_digits_with_underscores<F>(&mut self, is_digit: F) -> Result<DigitScan, LexError>
    where
        F: Fn(u8) -> bool,
    {
        let mut had_digit = false;
        let mut trailing_underscore = false;
        while let Some(c) = self.peek() {
            if c == b'_' {
                if !had_digit {
                    return Err(LexError {
                        message: "underscore must follow a digit".into(),
                        span: Span::new(self.pos, self.pos + 1),
                    });
                }
                trailing_underscore = true;
                self.pos += 1;
            } else if is_digit(c) {
                had_digit = true;
                trailing_underscore = false;
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(DigitScan {
            had_digit,
            trailing_underscore,
        })
    }
}

struct DigitScan {
    had_digit: bool,
    trailing_underscore: bool,
}

fn is_digit_in_base(c: u8, base: u32) -> bool {
    (c as char).is_digit(base)
}
