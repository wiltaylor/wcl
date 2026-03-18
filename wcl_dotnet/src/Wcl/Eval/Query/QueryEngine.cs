using System;
using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval.Query
{
    public class QueryEngine
    {
        public WclValue Execute(QueryPipeline pipeline, List<BlockRef> blocks,
                                Evaluator evaluator, ScopeId scope)
        {
            // Step 1: Select
            var selected = ApplySelector(pipeline.Selector, blocks);

            // Step 2: Apply filters
            WclValue result = WclValue.NewList(selected.Select(b => WclValue.NewBlockRef(b)).ToList());

            foreach (var filter in pipeline.Filters)
            {
                result = ApplyFilter(filter, result, evaluator, scope);
            }

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
            var current = blocks;
            foreach (var seg in segments)
            {
                switch (seg)
                {
                    case IdentPathSegment ips:
                        current = current.Where(b => b.Kind == ips.Ident.Name).ToList();
                        if (current.Count == 0)
                        {
                            // Try as child navigation
                            var children = new List<BlockRef>();
                            foreach (var b in blocks)
                                children.AddRange(b.Children.Where(c => c.Kind == ips.Ident.Name));
                            current = children;
                        }
                        break;
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
                    }
                    return WclValue.NewList(result);
                }
                case AttrComparisonFilter acf:
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef)
                        {
                            var br = item.AsBlockRef();
                            if (br.Attributes.TryGetValue(acf.Attr.Name, out var attrVal))
                            {
                                var filterVal = evaluator.EvalExpr(acf.Value, scope);
                                if (CompareValues(attrVal, acf.Op, filterVal))
                                    result.Add(item);
                            }
                        }
                    }
                    return WclValue.NewList(result);
                }
                case HasAttrFilter haf:
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef && item.AsBlockRef().Attributes.ContainsKey(haf.Attr.Name))
                            result.Add(item);
                    }
                    return WclValue.NewList(result);
                }
                case HasDecoratorFilter hdf:
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef && item.AsBlockRef().HasDecorator(hdf.Name.Name))
                            result.Add(item);
                    }
                    return WclValue.NewList(result);
                }
                case DecoratorArgFilterNode daf:
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        if (item.Kind == WclValueKind.BlockRef)
                        {
                            var dec = item.AsBlockRef().GetDecorator(daf.DecoratorName.Name);
                            if (dec != null && dec.Args.TryGetValue(daf.ParamName.Name, out var argVal))
                            {
                                var filterVal = evaluator.EvalExpr(daf.Value, scope);
                                if (CompareValues(argVal, daf.Op, filterVal))
                                    result.Add(item);
                            }
                        }
                    }
                    return WclValue.NewList(result);
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
                case BinOp.Lt:
                    if (left.Kind == WclValueKind.Int && right.Kind == WclValueKind.Int)
                        return left.AsInt() < right.AsInt();
                    return false;
                case BinOp.Gt:
                    if (left.Kind == WclValueKind.Int && right.Kind == WclValueKind.Int)
                        return left.AsInt() > right.AsInt();
                    return false;
                case BinOp.Lte:
                    if (left.Kind == WclValueKind.Int && right.Kind == WclValueKind.Int)
                        return left.AsInt() <= right.AsInt();
                    return false;
                case BinOp.Gte:
                    if (left.Kind == WclValueKind.Int && right.Kind == WclValueKind.Int)
                        return left.AsInt() >= right.AsInt();
                    return false;
                default: return false;
            }
        }
    }
}
