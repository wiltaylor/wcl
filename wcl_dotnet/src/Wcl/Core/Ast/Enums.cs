namespace Wcl.Core.Ast
{
    public enum BinOp
    {
        Add,   // +
        Sub,   // -
        Mul,   // *
        Div,   // /
        Mod,   // %
        Eq,    // ==
        Neq,   // !=
        Lt,    // <
        Gt,    // >
        Lte,   // <=
        Gte,   // >=
        Match, // =~
        And,   // &&
        Or,    // ||
    }

    public static class BinOpExtensions
    {
        public static int Precedence(this BinOp op) => op switch
        {
            BinOp.Or => 2,
            BinOp.And => 3,
            BinOp.Eq or BinOp.Neq => 4,
            BinOp.Lt or BinOp.Gt or BinOp.Lte or BinOp.Gte or BinOp.Match => 5,
            BinOp.Add or BinOp.Sub => 6,
            BinOp.Mul or BinOp.Div or BinOp.Mod => 7,
            _ => 0,
        };
    }

    public enum UnaryOp
    {
        Not, // !
        Neg, // -
    }

    public enum ImportKind
    {
        Relative,
        Library,
    }

    public enum MacroKind
    {
        Function,
        Attribute,
    }

    public enum DecoratorTarget
    {
        Block,
        Attribute,
        Table,
        Schema,
    }

    public enum ScopeKind
    {
        Module,
        Block,
        ForLoop,
        Lambda,
        Import,
    }
}
