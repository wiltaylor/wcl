using System.Collections.Generic;

namespace Wcl.Core.Ast
{
    public abstract class TypeExpr
    {
        public abstract Span GetSpan();
    }

    public sealed class StringTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public StringTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class IntTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public IntTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class FloatTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public FloatTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class BoolTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public BoolTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class NullTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public NullTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class IdentifierTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public IdentifierTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class AnyTypeExpr : TypeExpr
    {
        public Span Span { get; }
        public AnyTypeExpr(Span span) => Span = span;
        public override Span GetSpan() => Span;
    }

    public sealed class ListTypeExpr : TypeExpr
    {
        public TypeExpr Inner { get; }
        public Span Span { get; }
        public ListTypeExpr(TypeExpr inner, Span span) { Inner = inner; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class MapTypeExpr : TypeExpr
    {
        public TypeExpr KeyType { get; }
        public TypeExpr ValueType { get; }
        public Span Span { get; }
        public MapTypeExpr(TypeExpr keyType, TypeExpr valueType, Span span)
        { KeyType = keyType; ValueType = valueType; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class SetTypeExpr : TypeExpr
    {
        public TypeExpr Inner { get; }
        public Span Span { get; }
        public SetTypeExpr(TypeExpr inner, Span span) { Inner = inner; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class RefTypeExpr : TypeExpr
    {
        public StringLit SchemaName { get; }
        public Span Span { get; }
        public RefTypeExpr(StringLit schemaName, Span span) { SchemaName = schemaName; Span = span; }
        public override Span GetSpan() => Span;
    }

    public sealed class UnionTypeExpr : TypeExpr
    {
        public List<TypeExpr> Types { get; }
        public Span Span { get; }
        public UnionTypeExpr(List<TypeExpr> types, Span span) { Types = types; Span = span; }
        public override Span GetSpan() => Span;
    }
}
