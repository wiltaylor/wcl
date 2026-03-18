namespace Wcl.Core.Tokens
{
    public class Token
    {
        public TokenKind Kind { get; }
        public Span Span { get; }

        // Payload fields
        public string? StringValue { get; }
        public long IntValue { get; }
        public double DoubleValue { get; }
        public bool BoolValue { get; }

        // Heredoc flags
        public bool HeredocIndented { get; }
        public bool HeredocRaw { get; }

        private Token(TokenKind kind, Span span, string? stringValue = null,
                       long intValue = 0, double doubleValue = 0, bool boolValue = false,
                       bool heredocIndented = false, bool heredocRaw = false)
        {
            Kind = kind;
            Span = span;
            StringValue = stringValue;
            IntValue = intValue;
            DoubleValue = doubleValue;
            BoolValue = boolValue;
            HeredocIndented = heredocIndented;
            HeredocRaw = heredocRaw;
        }

        // Factory methods
        public static Token Ident(string name, Span span) =>
            new Token(TokenKind.Ident, span, stringValue: name);

        public static Token IdentifierLit(string value, Span span) =>
            new Token(TokenKind.IdentifierLit, span, stringValue: value);

        public static Token StringLiteral(string value, Span span) =>
            new Token(TokenKind.StringLit, span, stringValue: value);

        public static Token IntLiteral(long value, Span span) =>
            new Token(TokenKind.IntLit, span, intValue: value);

        public static Token FloatLiteral(double value, Span span) =>
            new Token(TokenKind.FloatLit, span, doubleValue: value);

        public static Token BoolLiteral(bool value, Span span) =>
            new Token(TokenKind.BoolLit, span, boolValue: value);

        public static Token NullLiteral(Span span) =>
            new Token(TokenKind.NullLit, span);

        public static Token HeredocLiteral(string content, bool indented, bool raw, Span span) =>
            new Token(TokenKind.Heredoc, span, stringValue: content,
                      heredocIndented: indented, heredocRaw: raw);

        public static Token Simple(TokenKind kind, Span span) =>
            new Token(kind, span);

        public static Token Comment(TokenKind kind, string text, Span span) =>
            new Token(kind, span, stringValue: text);

        public static Token EofToken(Span span) =>
            new Token(TokenKind.Eof, span);

        public override string ToString() =>
            StringValue != null ? $"{Kind}({StringValue})" : $"{Kind}";
    }
}
