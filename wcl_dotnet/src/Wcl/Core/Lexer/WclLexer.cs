using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using Wcl.Core.Tokens;

namespace Wcl.Core.Lexer
{
    public static class WclLexer
    {
        public static (List<Token> Tokens, DiagnosticBag Diagnostics) Lex(string source, FileId fileId)
        {
            // Strip UTF-8 BOM
            if (source.Length > 0 && source[0] == '\uFEFF')
                source = source.Substring(1);

            var lexer = new LexerState(source, fileId);
            var tokens = new List<Token>();

            while (lexer.Pos < lexer.Input.Length)
            {
                var tok = lexer.NextToken();
                if (tok != null)
                    tokens.Add(tok);
            }

            tokens.Add(Token.EofToken(new Span(fileId, lexer.Pos, lexer.Pos)));
            return (tokens, lexer.Diagnostics);
        }

        private class LexerState
        {
            public string Input;
            public int Pos;
            public FileId File;
            public DiagnosticBag Diagnostics = new DiagnosticBag();

            public LexerState(string input, FileId file)
            {
                Input = input;
                Pos = 0;
                File = file;
            }

            public char? Peek()
            {
                if (Pos < Input.Length) return Input[Pos];
                return null;
            }

            public char? Peek2()
            {
                if (Pos + 1 < Input.Length) return Input[Pos + 1];
                return null;
            }

            public bool StartsWith(string s)
            {
                if (Pos + s.Length > Input.Length) return false;
                for (int i = 0; i < s.Length; i++)
                    if (Input[Pos + i] != s[i]) return false;
                return true;
            }

            public void Advance(int count) => Pos += count;

            public char? AdvanceChar()
            {
                if (Pos >= Input.Length) return null;
                return Input[Pos++];
            }

            private Token MakeTok(TokenKind kind, int start) =>
                Token.Simple(kind, new Span(File, start, Pos));

            public void SkipInlineWhitespace()
            {
                while (Pos < Input.Length)
                {
                    char c = Input[Pos];
                    if (c == ' ' || c == '\t')
                        Pos++;
                    else
                        break;
                }
            }

            public Token? NextToken()
            {
                SkipInlineWhitespace();
                if (Pos >= Input.Length) return null;

                int start = Pos;
                char c = Input[Pos];

                // Newlines
                if (c == '\n')
                {
                    Pos++;
                    return MakeTok(TokenKind.Newline, start);
                }
                if (c == '\r')
                {
                    Pos++;
                    if (Pos < Input.Length && Input[Pos] == '\n') Pos++;
                    return MakeTok(TokenKind.Newline, start);
                }

                // Comments
                if (StartsWith("///")) return LexDocComment(start);
                if (StartsWith("//")) return LexLineComment(start);
                if (StartsWith("/*")) return LexBlockComment(start);

                // Heredoc
                if (StartsWith("<<")) return LexHeredoc(start);

                // String
                if (c == '"') return LexString(start);

                // Numbers
                if (char.IsDigit(c)) return LexNumber(start);

                // Identifiers / keywords
                if (char.IsLetter(c) || c == '_') return LexIdentOrKeyword(start);

                // Multi-char operators
                if (StartsWith("${")) { Advance(2); return Token.Simple(TokenKind.Eof, new Span(File, start, Pos)); } // InterpStart not needed at top-level lex
                if (StartsWith("==")) { Advance(2); return MakeTok(TokenKind.EqEq, start); }
                if (StartsWith("!=")) { Advance(2); return MakeTok(TokenKind.Neq, start); }
                if (StartsWith("<=")) { Advance(2); return MakeTok(TokenKind.Lte, start); }
                if (StartsWith(">=")) { Advance(2); return MakeTok(TokenKind.Gte, start); }
                if (StartsWith("=~")) { Advance(2); return MakeTok(TokenKind.Match, start); }
                if (StartsWith("&&")) { Advance(2); return MakeTok(TokenKind.And, start); }
                if (StartsWith("||")) { Advance(2); return MakeTok(TokenKind.Or, start); }
                if (StartsWith("=>")) { Advance(2); return MakeTok(TokenKind.FatArrow, start); }
                if (StartsWith("..")) { Advance(2); return MakeTok(TokenKind.DotDot, start); }

                // Single-char tokens
                Pos++;
                switch (c)
                {
                    case '{': return MakeTok(TokenKind.LBrace, start);
                    case '}': return MakeTok(TokenKind.RBrace, start);
                    case '[': return MakeTok(TokenKind.LBracket, start);
                    case ']': return MakeTok(TokenKind.RBracket, start);
                    case '(': return MakeTok(TokenKind.LParen, start);
                    case ')': return MakeTok(TokenKind.RParen, start);
                    case '=': return MakeTok(TokenKind.Equals, start);
                    case ',': return MakeTok(TokenKind.Comma, start);
                    case '|': return MakeTok(TokenKind.Pipe, start);
                    case '.': return MakeTok(TokenKind.Dot, start);
                    case '#': return MakeTok(TokenKind.Hash, start);
                    case '@': return MakeTok(TokenKind.At, start);
                    case ':': return MakeTok(TokenKind.Colon, start);
                    case '?': return MakeTok(TokenKind.Question, start);
                    case ';': return MakeTok(TokenKind.Newline, start); // semicolons treated as newlines
                    case '+': return MakeTok(TokenKind.Plus, start);
                    case '-': return MakeTok(TokenKind.Minus, start);
                    case '*': return MakeTok(TokenKind.Star, start);
                    case '/': return MakeTok(TokenKind.Slash, start);
                    case '%': return MakeTok(TokenKind.Percent, start);
                    case '<': return MakeTok(TokenKind.Lt, start);
                    case '>': return MakeTok(TokenKind.Gt, start);
                    case '!': return MakeTok(TokenKind.Not, start);
                    default:
                        Diagnostics.Error($"unexpected character: '{c}'", new Span(File, start, Pos));
                        return NextToken();
                }
            }

            private Token LexDocComment(int start)
            {
                int textStart = Pos;
                while (Pos < Input.Length && Input[Pos] != '\n' && Input[Pos] != '\r')
                    Pos++;
                string text = Input.Substring(textStart, Pos - textStart);
                return Token.Comment(TokenKind.DocComment, text, new Span(File, start, Pos));
            }

            private Token LexLineComment(int start)
            {
                int textStart = Pos;
                while (Pos < Input.Length && Input[Pos] != '\n' && Input[Pos] != '\r')
                    Pos++;
                string text = Input.Substring(textStart, Pos - textStart);
                return Token.Comment(TokenKind.LineComment, text, new Span(File, start, Pos));
            }

            private Token LexBlockComment(int start)
            {
                int textStart = Pos;
                Advance(2); // consume /*
                int depth = 1;
                while (depth > 0)
                {
                    if (Pos >= Input.Length)
                    {
                        Diagnostics.Error("unterminated block comment", new Span(File, start, Pos));
                        break;
                    }
                    if (StartsWith("/*")) { Advance(2); depth++; }
                    else if (StartsWith("*/")) { Advance(2); depth--; }
                    else Pos++;
                }
                string text = Input.Substring(textStart, Pos - textStart);
                return Token.Comment(TokenKind.BlockComment, text, new Span(File, start, Pos));
            }

            private Token LexString(int start)
            {
                Pos++; // consume opening "
                var content = new StringBuilder();
                while (true)
                {
                    if (Pos >= Input.Length)
                    {
                        Diagnostics.ErrorWithCode("E003", "unterminated string literal", new Span(File, start, Pos));
                        break;
                    }
                    char ch = Input[Pos];
                    if (ch == '"') { Pos++; break; }
                    if (ch == '\\')
                    {
                        Pos++; // consume backslash
                        if (Pos >= Input.Length)
                        {
                            Diagnostics.Error("unexpected end of file in escape sequence", new Span(File, start, Pos));
                            break;
                        }
                        char esc = Input[Pos++];
                        switch (esc)
                        {
                            case '\\': content.Append('\\'); break;
                            case '"': content.Append('"'); break;
                            case 'n': content.Append('\n'); break;
                            case 'r': content.Append('\r'); break;
                            case 't': content.Append('\t'); break;
                            case 'u':
                                var uch = LexUnicodeEscape(4, start);
                                if (uch.HasValue) content.Append(uch.Value);
                                break;
                            case 'U':
                                var Uch = LexUnicodeEscape(8, start);
                                if (Uch.HasValue) content.Append(Uch.Value);
                                break;
                            default:
                                Diagnostics.Error($"unknown escape sequence: \\{esc}", new Span(File, Pos - 2, Pos));
                                content.Append('\\');
                                content.Append(esc);
                                break;
                        }
                    }
                    else if (ch == '$' && Pos + 1 < Input.Length && Input[Pos + 1] == '{')
                    {
                        // Preserve ${  for parser to handle interpolation
                        content.Append('$');
                        Pos++;
                        content.Append('{');
                        Pos++;
                    }
                    else
                    {
                        content.Append(ch);
                        Pos++;
                    }
                }
                return Token.StringLiteral(content.ToString(), new Span(File, start, Pos));
            }

            private char? LexUnicodeEscape(int digits, int errStart)
            {
                var hex = new StringBuilder(digits);
                for (int i = 0; i < digits; i++)
                {
                    if (Pos >= Input.Length || !IsHexDigit(Input[Pos]))
                    {
                        Diagnostics.Error($"expected {digits} hex digits in unicode escape", new Span(File, errStart, Pos));
                        return null;
                    }
                    hex.Append(Input[Pos++]);
                }
                uint code = uint.Parse(hex.ToString(), NumberStyles.HexNumber);
                try
                {
                    char result = Convert.ToChar((int)code);
                    return result;
                }
                catch
                {
                    // Try surrogate pair range or invalid
                    if (code <= 0x10FFFF)
                        return (char)code; // Will work for BMP
                    Diagnostics.Error($"invalid unicode code point: U+{code:X}", new Span(File, errStart, Pos));
                    return null;
                }
            }

            private static bool IsHexDigit(char c) =>
                (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');

            private Token LexHeredoc(int start)
            {
                Advance(2); // consume <<
                bool indented = false;
                if (Pos < Input.Length && Input[Pos] == '-') { Pos++; indented = true; }
                bool raw = false;
                if (Pos < Input.Length && Input[Pos] == '\'') { Pos++; raw = true; }

                int tagStart = Pos;
                while (Pos < Input.Length && (char.IsLetterOrDigit(Input[Pos]) || Input[Pos] == '_'))
                    Pos++;
                string tag = Input.Substring(tagStart, Pos - tagStart);

                if (raw && Pos < Input.Length && Input[Pos] == '\'') Pos++;

                if (tag.Length == 0)
                {
                    Diagnostics.Error("heredoc delimiter tag is empty", new Span(File, start, Pos));
                    return Token.HeredocLiteral("", indented, raw, new Span(File, start, Pos));
                }

                // Skip to end of opening line
                while (Pos < Input.Length)
                {
                    if (Input[Pos] == '\n') { Pos++; break; }
                    if (Input[Pos] == '\r') { Pos++; if (Pos < Input.Length && Input[Pos] == '\n') Pos++; break; }
                    Pos++;
                }

                var lines = new List<string>();
                int closingIndent = 0;
                bool foundClose = false;

                while (Pos < Input.Length)
                {
                    int lineStart = Pos;
                    while (Pos < Input.Length && Input[Pos] != '\n' && Input[Pos] != '\r')
                        Pos++;
                    string line = Input.Substring(lineStart, Pos - lineStart);

                    // Consume newline
                    if (Pos < Input.Length && Input[Pos] == '\r') Pos++;
                    if (Pos < Input.Length && Input[Pos] == '\n') Pos++;

                    string trimmedStart = line.TrimStart();
                    if (trimmedStart == tag || trimmedStart == tag.TrimEnd())
                    {
                        closingIndent = line.Length - line.TrimStart().Length;
                        foundClose = true;
                        break;
                    }
                    lines.Add(line);
                }

                if (!foundClose)
                    Diagnostics.ErrorWithCode("E003", $"unterminated heredoc (expected closing {tag})", new Span(File, start, Pos));

                string content;
                if (indented && closingIndent > 0)
                {
                    var sb = new StringBuilder();
                    for (int i = 0; i < lines.Count; i++)
                    {
                        if (i > 0) sb.Append('\n');
                        string l = lines[i];
                        sb.Append(l.Length >= closingIndent ? l.Substring(closingIndent) : l.TrimStart());
                    }
                    content = sb.ToString();
                }
                else
                {
                    content = string.Join("\n", lines);
                }

                if (!raw)
                    content = ProcessHeredocEscapes(content, start);

                return Token.HeredocLiteral(content, indented, raw, new Span(File, start, Pos));
            }

            private string ProcessHeredocEscapes(string s, int errStart)
            {
                var sb = new StringBuilder(s.Length);
                int i = 0;
                while (i < s.Length)
                {
                    if (s[i] == '\\' && i + 1 < s.Length)
                    {
                        char next = s[i + 1];
                        switch (next)
                        {
                            case '\\': sb.Append('\\'); i += 2; break;
                            case '"': sb.Append('"'); i += 2; break;
                            case 'n': sb.Append('\n'); i += 2; break;
                            case 'r': sb.Append('\r'); i += 2; break;
                            case 't': sb.Append('\t'); i += 2; break;
                            case 'u':
                            {
                                i += 2;
                                var hex = new StringBuilder();
                                for (int j = 0; j < 4 && i < s.Length && IsHexDigit(s[i]); j++)
                                    hex.Append(s[i++]);
                                if (hex.Length == 4)
                                {
                                    uint code = uint.Parse(hex.ToString(), NumberStyles.HexNumber);
                                    try { sb.Append(Convert.ToChar((int)code)); }
                                    catch { Diagnostics.Error($"invalid unicode code point: U+{code:X}", new Span(File, errStart, Pos)); }
                                }
                                else { sb.Append('\\'); sb.Append('u'); sb.Append(hex); }
                                break;
                            }
                            case 'U':
                            {
                                i += 2;
                                var hex = new StringBuilder();
                                for (int j = 0; j < 8 && i < s.Length && IsHexDigit(s[i]); j++)
                                    hex.Append(s[i++]);
                                if (hex.Length == 8)
                                {
                                    uint code = uint.Parse(hex.ToString(), NumberStyles.HexNumber);
                                    try { sb.Append(Convert.ToChar((int)code)); }
                                    catch { Diagnostics.Error($"invalid unicode code point: U+{code:X}", new Span(File, errStart, Pos)); }
                                }
                                else { sb.Append('\\'); sb.Append('U'); sb.Append(hex); }
                                break;
                            }
                            default: sb.Append('\\'); sb.Append(next); i += 2; break;
                        }
                    }
                    else
                    {
                        sb.Append(s[i++]);
                    }
                }
                return sb.ToString();
            }

            private Token LexNumber(int start)
            {
                // Hex
                if (StartsWith("0x") || StartsWith("0X"))
                {
                    Advance(2);
                    int hexStart = Pos;
                    while (Pos < Input.Length && (IsHexDigit(Input[Pos]) || Input[Pos] == '_')) Pos++;
                    string clean = RemoveUnderscores(Input.Substring(hexStart, Pos - hexStart));
                    if (long.TryParse(clean, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out long hv))
                        return Token.IntLiteral(hv, new Span(File, start, Pos));
                    Diagnostics.Error($"invalid hexadecimal literal: 0x{clean}", new Span(File, start, Pos));
                    return Token.IntLiteral(0, new Span(File, start, Pos));
                }

                // Octal
                if (StartsWith("0o") || StartsWith("0O"))
                {
                    Advance(2);
                    int octStart = Pos;
                    while (Pos < Input.Length && ((Input[Pos] >= '0' && Input[Pos] <= '7') || Input[Pos] == '_')) Pos++;
                    string clean = RemoveUnderscores(Input.Substring(octStart, Pos - octStart));
                    try
                    {
                        long ov = Convert.ToInt64(clean, 8);
                        return Token.IntLiteral(ov, new Span(File, start, Pos));
                    }
                    catch
                    {
                        Diagnostics.Error($"invalid octal literal: 0o{clean}", new Span(File, start, Pos));
                        return Token.IntLiteral(0, new Span(File, start, Pos));
                    }
                }

                // Binary
                if (StartsWith("0b") || StartsWith("0B"))
                {
                    Advance(2);
                    int binStart = Pos;
                    while (Pos < Input.Length && (Input[Pos] == '0' || Input[Pos] == '1' || Input[Pos] == '_')) Pos++;
                    string clean = RemoveUnderscores(Input.Substring(binStart, Pos - binStart));
                    try
                    {
                        long bv = Convert.ToInt64(clean, 2);
                        return Token.IntLiteral(bv, new Span(File, start, Pos));
                    }
                    catch
                    {
                        Diagnostics.Error($"invalid binary literal: 0b{clean}", new Span(File, start, Pos));
                        return Token.IntLiteral(0, new Span(File, start, Pos));
                    }
                }

                // Decimal int or float
                int numStart = Pos;
                while (Pos < Input.Length && (char.IsDigit(Input[Pos]) || Input[Pos] == '_')) Pos++;

                // Check for float
                bool isFloat = Pos < Input.Length && Input[Pos] == '.'
                    && Pos + 1 < Input.Length && char.IsDigit(Input[Pos + 1]);

                if (isFloat)
                {
                    Pos++; // consume .
                    while (Pos < Input.Length && (char.IsDigit(Input[Pos]) || Input[Pos] == '_')) Pos++;
                    // Optional exponent
                    if (Pos < Input.Length && (Input[Pos] == 'e' || Input[Pos] == 'E'))
                    {
                        Pos++;
                        if (Pos < Input.Length && (Input[Pos] == '+' || Input[Pos] == '-')) Pos++;
                        while (Pos < Input.Length && (char.IsDigit(Input[Pos]) || Input[Pos] == '_')) Pos++;
                    }
                    string fclean = RemoveUnderscores(Input.Substring(numStart, Pos - numStart));
                    if (double.TryParse(fclean, NumberStyles.Float, CultureInfo.InvariantCulture, out double fv))
                        return Token.FloatLiteral(fv, new Span(File, start, Pos));
                    Diagnostics.Error($"invalid float literal: {fclean}", new Span(File, start, Pos));
                    return Token.FloatLiteral(0.0, new Span(File, start, Pos));
                }

                string iclean = RemoveUnderscores(Input.Substring(numStart, Pos - numStart));
                if (long.TryParse(iclean, NumberStyles.Integer, CultureInfo.InvariantCulture, out long iv))
                    return Token.IntLiteral(iv, new Span(File, start, Pos));
                Diagnostics.Error($"integer literal out of range: {iclean}", new Span(File, start, Pos));
                return Token.IntLiteral(0, new Span(File, start, Pos));
            }

            private static string RemoveUnderscores(string s)
            {
                if (s.IndexOf('_') < 0) return s;
                var sb = new StringBuilder(s.Length);
                foreach (char c in s)
                    if (c != '_') sb.Append(c);
                return sb.ToString();
            }

            private Token LexIdentOrKeyword(int start)
            {
                int wordStart = Pos;
                while (Pos < Input.Length)
                {
                    char ch = Input[Pos];
                    if (char.IsLetterOrDigit(ch) || ch == '_')
                    {
                        Pos++;
                    }
                    else if (ch == '-')
                    {
                        // Only consume hyphen if followed by alphanumeric/_
                        if (Pos + 1 < Input.Length && (char.IsLetterOrDigit(Input[Pos + 1]) || Input[Pos + 1] == '_'))
                            Pos++;
                        else
                            break;
                    }
                    else
                    {
                        break;
                    }
                }
                string word = Input.Substring(wordStart, Pos - wordStart);

                switch (word)
                {
                    case "let": return MakeTok(TokenKind.Let, start);
                    case "partial": return MakeTok(TokenKind.Partial, start);
                    case "macro": return MakeTok(TokenKind.Macro, start);
                    case "schema": return MakeTok(TokenKind.Schema, start);
                    case "table": return MakeTok(TokenKind.Table, start);
                    case "import": return MakeTok(TokenKind.Import, start);
                    case "export": return MakeTok(TokenKind.Export, start);
                    case "query": return MakeTok(TokenKind.Query, start);
                    case "ref": return MakeTok(TokenKind.Ref, start);
                    case "for": return MakeTok(TokenKind.For, start);
                    case "in": return MakeTok(TokenKind.In, start);
                    case "true": return Token.BoolLiteral(true, new Span(File, start, Pos));
                    case "false": return Token.BoolLiteral(false, new Span(File, start, Pos));
                    case "null": return Token.NullLiteral(new Span(File, start, Pos));
                    case "if": return MakeTok(TokenKind.If, start);
                    case "else": return MakeTok(TokenKind.Else, start);
                    case "when": return MakeTok(TokenKind.When, start);
                    case "inject": return MakeTok(TokenKind.Inject, start);
                    case "set": return MakeTok(TokenKind.Set, start);
                    case "remove": return MakeTok(TokenKind.Remove, start);
                    case "self": return MakeTok(TokenKind.SelfKw, start);
                    case "validation": return MakeTok(TokenKind.Validation, start);
                    case "decorator_schema": return MakeTok(TokenKind.DecoratorSchema, start);
                    case "declare": return MakeTok(TokenKind.Declare, start);
                    default:
                        if (word.Contains("-"))
                            return Token.IdentifierLit(word, new Span(File, start, Pos));
                        return Token.Ident(word, new Span(File, start, Pos));
                }
            }
        }
    }
}
