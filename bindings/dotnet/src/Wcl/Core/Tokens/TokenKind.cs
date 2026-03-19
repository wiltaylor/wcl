namespace Wcl.Core.Tokens
{
    public enum TokenKind
    {
        // Literals
        Ident,
        IdentifierLit,
        StringLit,
        IntLit,
        FloatLit,
        BoolLit,
        NullLit,
        Heredoc,

        // Keywords
        Let,
        Partial,
        Macro,
        Schema,
        Table,
        Import,
        Export,
        Query,
        Ref,
        For,
        In,
        If,
        Else,
        When,
        Inject,
        Set,
        Remove,
        SelfKw,
        Validation,
        DecoratorSchema,
        Declare,

        // Delimiters
        LBrace,
        RBrace,
        LBracket,
        RBracket,
        LParen,
        RParen,

        // Punctuation
        Equals,
        Comma,
        Pipe,
        Dot,
        DotDot,
        Colon,
        At,
        Hash,
        Question,
        FatArrow,

        // Operators
        Plus,
        Minus,
        Star,
        Slash,
        Percent,
        EqEq,
        Neq,
        Lt,
        Gt,
        Lte,
        Gte,
        Match,
        Not,
        And,
        Or,

        // Special
        Newline,
        LineComment,
        BlockComment,
        DocComment,
        Eof,
    }
}
