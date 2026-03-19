using System.Collections.Generic;

namespace Wcl.Core.Ast
{
    public class QueryPipeline
    {
        public QuerySelector Selector { get; set; }
        public List<QueryFilter> Filters { get; set; }
        public Span Span { get; set; }
        public QueryPipeline(QuerySelector selector, List<QueryFilter> filters, Span span)
        { Selector = selector; Filters = filters; Span = span; }
    }

    // Selectors
    public abstract class QuerySelector { }

    public sealed class KindSelector : QuerySelector
    {
        public Ident Kind { get; }
        public KindSelector(Ident kind) => Kind = kind;
    }

    public sealed class KindIdSelector : QuerySelector
    {
        public Ident Kind { get; }
        public IdentifierLit Id { get; }
        public KindIdSelector(Ident kind, IdentifierLit id) { Kind = kind; Id = id; }
    }

    public sealed class KindLabelSelector : QuerySelector
    {
        public Ident Kind { get; }
        public StringLit Label { get; }
        public KindLabelSelector(Ident kind, StringLit label) { Kind = kind; Label = label; }
    }

    public sealed class PathSelector : QuerySelector
    {
        public List<PathSegment> Segments { get; }
        public PathSelector(List<PathSegment> segments) => Segments = segments;
    }

    public sealed class RecursiveSelector : QuerySelector
    {
        public Ident Kind { get; }
        public RecursiveSelector(Ident kind) => Kind = kind;
    }

    public sealed class RecursiveIdSelector : QuerySelector
    {
        public Ident Kind { get; }
        public IdentifierLit Id { get; }
        public RecursiveIdSelector(Ident kind, IdentifierLit id) { Kind = kind; Id = id; }
    }

    public sealed class RootSelector : QuerySelector { }

    public sealed class WildcardSelector : QuerySelector { }

    public sealed class TableLabelSelector : QuerySelector
    {
        public StringLit Label { get; }
        public TableLabelSelector(StringLit label) => Label = label;
    }

    public sealed class TableIdSelector : QuerySelector
    {
        public IdentifierLit Id { get; }
        public TableIdSelector(IdentifierLit id) => Id = id;
    }

    // Path segments
    public abstract class PathSegment { }
    public sealed class IdentPathSegment : PathSegment
    {
        public Ident Ident { get; }
        public IdentPathSegment(Ident ident) => Ident = ident;
    }
    public sealed class StringLabelPathSegment : PathSegment
    {
        public StringLit Label { get; }
        public StringLabelPathSegment(StringLit label) => Label = label;
    }

    // Filters
    public abstract class QueryFilter { }

    public sealed class AttrComparisonFilter : QueryFilter
    {
        public Ident Attr { get; }
        public BinOp Op { get; }
        public Expr Value { get; }
        public AttrComparisonFilter(Ident attr, BinOp op, Expr value) { Attr = attr; Op = op; Value = value; }
    }

    public sealed class ProjectionFilter : QueryFilter
    {
        public Ident Attr { get; }
        public ProjectionFilter(Ident attr) => Attr = attr;
    }

    public sealed class HasAttrFilter : QueryFilter
    {
        public Ident Attr { get; }
        public HasAttrFilter(Ident attr) => Attr = attr;
    }

    public sealed class HasDecoratorFilter : QueryFilter
    {
        public Ident Name { get; }
        public HasDecoratorFilter(Ident name) => Name = name;
    }

    public sealed class DecoratorArgFilterNode : QueryFilter
    {
        public Ident DecoratorName { get; }
        public Ident ParamName { get; }
        public BinOp Op { get; }
        public Expr Value { get; }
        public DecoratorArgFilterNode(Ident decoratorName, Ident paramName, BinOp op, Expr value)
        { DecoratorName = decoratorName; ParamName = paramName; Op = op; Value = value; }
    }
}
