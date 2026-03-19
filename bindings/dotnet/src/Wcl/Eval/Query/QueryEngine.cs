using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval.Query
{
    public class QueryEngine
    {
        public WclValue Execute(QueryPipeline pipeline, List<BlockRef> blocks,
                                Evaluator evaluator, ScopeId scope)
        {
            var selected = ApplySelector(pipeline.Selector, blocks);
            WclValue result = WclValue.NewList(selected.Select(b => WclValue.NewBlockRef(b)).ToList());

            foreach (var filter in pipeline.Filters)
                result = ApplyFilter(filter, result, evaluator, scope);

            return result;
        }

        private List<BlockRef> ApplySelector(QuerySelector selector, List<BlockRef> blocks)
        {
            switch (selector)
            {
                case KindSelector ks:
                    return blocks.Where(b => b.Kind == ks.Kind.Name).ToList();
                case KindIdSelector kis:
                    return blocks.Where(b => b.Kind == kis.Kind.Name && b.Id == kis.Id.Value).ToList();
                case KindLabelSelector kls:
                {
                    var label = GetStringLitValue(kls.Label);
                    return blocks.Where(b => b.Kind == kls.Kind.Name && b.Labels.Contains(label)).ToList();
                }
                case RecursiveSelector rs:
                    return CollectRecursive(blocks, b => b.Kind == rs.Kind.Name);
                case RecursiveIdSelector ris:
                    return CollectRecursive(blocks, b => b.Kind == ris.Kind.Name && b.Id == ris.Id.Value);
                case WildcardSelector _:
                    return new List<BlockRef>(blocks);
                case RootSelector _:
                    return new List<BlockRef>(blocks);
                case PathSelector ps:
                    return ResolvePath(ps.Segments, blocks);
                case TableLabelSelector tls:
                {
                    var label = GetStringLitValue(tls.Label);
                    // Tables don't have a "kind" per se; find blocks with matching labels
                    return blocks.Where(b => b.Labels.Contains(label)).ToList();
                }
                case TableIdSelector tis:
                    return blocks.Where(b => b.Id == tis.Id.Value).ToList();
                default:
                    return new List<BlockRef>();
            }
        }

        private List<BlockRef> CollectRecursive(List<BlockRef> blocks, Func<BlockRef, bool> pred)
        {
            var result = new List<BlockRef>();
            void Walk(List<BlockRef> bs)
            {
                foreach (var b in bs)
                {
                    if (pred(b)) result.Add(b);
                    Walk(b.Children);
                }
            }
            Walk(blocks);
            return result;
        }

        private List<BlockRef> ResolvePath(List<PathSegment> segments, List<BlockRef> blocks)
        {
            var current = new List<BlockRef>(blocks);

            for (int si = 0; si < segments.Count; si++)
            {
                var seg = segments[si];
                switch (seg)
                {
                    case IdentPathSegment ips:
                    {
                        if (si == 0)
                        {
                            // First segment: filter top-level blocks by kind
                            var matching = current.Where(b => b.Kind == ips.Ident.Name).ToList();
                            if (matching.Count > 0)
                            {
                                current = matching;
                            }
                            else
                            {
                                current = new List<BlockRef>();
                            }
                        }
                        else
                        {
                            // Subsequent segments: navigate into children
                            var children = new List<BlockRef>();
                            foreach (var b in current)
                            {
                                // Check children by kind
                                children.AddRange(b.Children.Where(c => c.Kind == ips.Ident.Name));
                                // Also check attributes that are block refs
                                if (b.Attributes.TryGetValue(ips.Ident.Name, out var attrVal))
                                {
                                    if (attrVal.Kind == WclValueKind.BlockRef)
                                        children.Add(attrVal.AsBlockRef());
                                }
                            }
                            current = children;
                        }
                        break;
                    }
                    case StringLabelPathSegment slps:
                    {
                        var label = GetStringLitValue(slps.Label);
                        // Filter by label within children
                        var labeled = new List<BlockRef>();
                        foreach (var b in current)
                        {
                            if (b.Labels.Contains(label))
                                labeled.Add(b);
                            labeled.AddRange(b.Children.Where(c => c.Labels.Contains(label)));
                        }
                        current = labeled;
                        break;
                    }
                }
            }
            return current;
        }

        private WclValue ApplyFilter(QueryFilter filter, WclValue input, Evaluator evaluator, ScopeId scope)
        {
            if (input.Kind != WclValueKind.List) return input;
            var list = input.AsList();

            switch (filter)
            {
                case ProjectionFilter pf:
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef)
                        {
                            var br = item.AsBlockRef();
                            if (br.Attributes.TryGetValue(pf.Attr.Name, out var val))
                                result.Add(val);
                        }
                        else if (item.Kind == WclValueKind.Map)
                        {
                            if (item.AsMap().TryGetValue(pf.Attr.Name, out var val))
                                result.Add(val);
                        }
                    }
                    return WclValue.NewList(result);
                }
                case AttrComparisonFilter acf:
                {
                    var filterVal = evaluator.EvalExpr(acf.Value, scope);
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef)
                        {
                            var br = item.AsBlockRef();
                            if (br.Attributes.TryGetValue(acf.Attr.Name, out var attrVal) &&
                                CompareValues(attrVal, acf.Op, filterVal))
                                result.Add(item);
                        }
                    }
                    return WclValue.NewList(result);
                }
                case HasAttrFilter haf:
                    return WclValue.NewList(list.Where(item =>
                        item.Kind == WclValueKind.BlockRef && item.AsBlockRef().Attributes.ContainsKey(haf.Attr.Name)).ToList());
                case HasDecoratorFilter hdf:
                    return WclValue.NewList(list.Where(item =>
                        item.Kind == WclValueKind.BlockRef && item.AsBlockRef().HasDecorator(hdf.Name.Name)).ToList());
                case DecoratorArgFilterNode daf:
                {
                    var filterVal = evaluator.EvalExpr(daf.Value, scope);
                    return WclValue.NewList(list.Where(item =>
                    {
                        if (item.Kind != WclValueKind.BlockRef) return false;
                        var dec = item.AsBlockRef().GetDecorator(daf.DecoratorName.Name);
                        return dec != null && dec.Args.TryGetValue(daf.ParamName.Name, out var argVal) &&
                               CompareValues(argVal, daf.Op, filterVal);
                    }).ToList());
                }
                default:
                    return input;
            }
        }

        private static bool CompareValues(WclValue left, BinOp op, WclValue right)
        {
            switch (op)
            {
                case BinOp.Eq: return left.Equals(right);
                case BinOp.Neq: return !left.Equals(right);
                case BinOp.Match:
                    return left.Kind == WclValueKind.String && right.Kind == WclValueKind.String &&
                           Regex.IsMatch(left.AsString(), right.AsString());
                default:
                {
                    // Numeric comparison with int/float promotion
                    double? a = left.Kind == WclValueKind.Int ? left.AsInt() :
                                left.Kind == WclValueKind.Float ? left.AsFloat() : (double?)null;
                    double? b = right.Kind == WclValueKind.Int ? right.AsInt() :
                                right.Kind == WclValueKind.Float ? right.AsFloat() : (double?)null;
                    if (a.HasValue && b.HasValue)
                    {
                        return op switch
                        {
                            BinOp.Lt => a.Value < b.Value,
                            BinOp.Gt => a.Value > b.Value,
                            BinOp.Lte => a.Value <= b.Value,
                            BinOp.Gte => a.Value >= b.Value,
                            _ => false,
                        };
                    }
                    // String comparison
                    if (left.Kind == WclValueKind.String && right.Kind == WclValueKind.String)
                    {
                        int cmp = string.Compare(left.AsString(), right.AsString(), StringComparison.Ordinal);
                        return op switch
                        {
                            BinOp.Lt => cmp < 0,
                            BinOp.Gt => cmp > 0,
                            BinOp.Lte => cmp <= 0,
                            BinOp.Gte => cmp >= 0,
                            _ => false,
                        };
                    }
                    return false;
                }
            }
        }

        private static string GetStringLitValue(StringLit sl)
        {
            if (sl.Parts.Count == 1 && sl.Parts[0] is LiteralPart lp) return lp.Value;
            var sb = new System.Text.StringBuilder();
            foreach (var p in sl.Parts)
                if (p is LiteralPart lp2) sb.Append(lp2.Value);
            return sb.ToString();
        }
    }
}
