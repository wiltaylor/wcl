using System.Collections.Generic;

namespace Wcl.Core.Ast
{
    public abstract class Expr
    {
        public abstract Span GetSpan();
    }

    public sealed class IntLitExpr : Expr
    {
        public long Value { get; }
        public Span Span { get; }
        public IntLitExpr(long value, Span span) { Value = value; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class FloatLitExpr : Expr
    {
        public double Value { get; }
        public Span Span { get; }
        public FloatLitExpr(double value, Span span) { Value = value; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class StringLitExpr : Expr
    {
        public StringLit StringLit { get; }
        public StringLitExpr(StringLit stringLit) => StringLit = stringLit;
        public override Span GetSpan() => StringLit.Span;
    }

    public sealed class BoolLitExpr : Expr
    {
        public bool Value { get; }
        public Span Span { get; }
        public BoolLitExpr(bool value, Span span) { Value = value; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class NullLitExpr : Expr
    {
        public Span Span { get; }
        public NullLitExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class IdentExpr : Expr
    {
        public Ident Ident { get; }
        public IdentExpr(Ident ident) => Ident = ident;
        public override Span GetSpan() => Ident.Span;
    }

    public sealed class IdentifierLitExpr : Expr
    {
        public IdentifierLit Lit { get; }
        public IdentifierLitExpr(IdentifierLit lit) => Lit = lit;
        public override Span GetSpan() => Lit.Span;
    }

    public sealed class ListExpr : Expr
    {
        public List<Expr> Items { get; }
        public Span Span { get; }
        public ListExpr(List<Expr> items, Span span) { Items = items; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class MapExpr : Expr
    {
        public List<(MapKey Key, Expr Value)> Entries { get; }
        public Span Span { get; }
        public MapExpr(List<(MapKey, Expr)> entries, Span span) { Entries = entries; Span = span; }
        public override Span GetSpan() => Span;
    }

    public abstract class MapKey { }
    public sealed class IdentMapKey : MapKey
    {
        public Ident Ident { get; }
        public IdentMapKey(Ident ident) => Ident = ident;
    }
    public sealed class StringMapKey : MapKey
    {
        public StringLit StringLit { get; }
        public StringMapKey(StringLit stringLit) => StringLit = stringLit;
    }

    public sealed class BinaryOpExpr : Expr
    {
        public Expr Left { get; }
        public BinOp Op { get; }
        public Expr Right { get; }
        public Span Span { get; }
        public BinaryOpExpr(Expr left, BinOp op, Expr right, Span span)
        { Left = left; Op = op; Right = right; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class UnaryOpExpr : Expr
    {
        public UnaryOp Op { get; }
        public Expr Operand { get; }
        public Span Span { get; }
        public UnaryOpExpr(UnaryOp op, Expr operand, Span span) { Op = op; Operand = operand; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class TernaryExpr : Expr
    {
        public Expr Condition { get; }
        public Expr ThenExpr { get; }
        public Expr ElseExpr { get; }
        public Span Span { get; }
        public TernaryExpr(Expr condition, Expr thenExpr, Expr elseExpr, Span span)
        { Condition = condition; ThenExpr = thenExpr; ElseExpr = elseExpr; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class MemberAccessExpr : Expr
    {
        public Expr Object { get; }
        public Ident Member { get; }
        public Span Span { get; }
        public MemberAccessExpr(Expr obj, Ident member, Span span)
        { Object = obj; Member = member; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class IndexAccessExpr : Expr
    {
        public Expr Object { get; }
        public Expr Index { get; }
        public Span Span { get; }
        public IndexAccessExpr(Expr obj, Expr index, Span span) { Object = obj; Index = index; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class FnCallExpr : Expr
    {
        public Expr Callee { get; }
        public List<CallArg> Args { get; }
        public Span Span { get; }
        public FnCallExpr(Expr callee, List<CallArg> args, Span span)
        { Callee = callee; Args = args; Span = span; }
        public override Span GetSpan() => Span;
    }

    public abstract class CallArg { }
    public sealed class PositionalCallArg : CallArg
    {
        public Expr Value { get; }
        public PositionalCallArg(Expr value) => Value = value;
    }
    public sealed class NamedCallArg : CallArg
    {
        public Ident Name { get; }
        public Expr Value { get; }
        public NamedCallArg(Ident name, Expr value) { Name = name; Value = value; }
    }

    public sealed class LambdaExpr : Expr
    {
        public List<Ident> Params { get; }
        public Expr Body { get; }
        public Span Span { get; }
        public LambdaExpr(List<Ident> parms, Expr body, Span span)
        { Params = parms; Body = body; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class BlockExprNode : Expr
    {
        public List<LetBinding> Lets { get; }
        public Expr FinalExpr { get; }
        public Span Span { get; }
        public BlockExprNode(List<LetBinding> lets, Expr finalExpr, Span span)
        { Lets = lets; FinalExpr = finalExpr; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class QueryExpr : Expr
    {
        public QueryPipeline Pipeline { get; }
        public Span Span { get; }
        public QueryExpr(QueryPipeline pipeline, Span span) { Pipeline = pipeline; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class RefExpr : Expr
    {
        public IdentifierLit Id { get; }
        public Span Span { get; }
        public RefExpr(IdentifierLit id, Span span) { Id = id; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class ImportRawExpr : Expr
    {
        public StringLit Path { get; }
        public Span Span { get; }
        public ImportRawExpr(StringLit path, Span span) { Path = path; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class ImportTableExpr : Expr
    {
        public StringLit Path { get; }
        public StringLit? Separator { get; }
        public Span Span { get; }
        public ImportTableExpr(StringLit path, StringLit? separator, Span span)
        { Path = path; Separator = separator; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class ParenExpr : Expr
    {
        public Expr Inner { get; }
        public Span Span { get; }
        public ParenExpr(Expr inner, Span span) { Inner = inner; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class SetExpr : Expr
    {
        public List<Expr> Items { get; }
        public Span Span { get; }
        public SetExpr(List<Expr> items, Span span) { Items = items; Span = span; }
        public override Span GetSpan() => Span;
    }
}
