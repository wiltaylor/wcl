using System;
using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval.ControlFlow
{
    public class ControlFlowExpander
    {
        private readonly uint _maxLoopDepth;
        private readonly uint _maxIterations;
        private uint _totalIterations;
        private readonly DiagnosticBag _diagnostics = new DiagnosticBag();

        public ControlFlowExpander(uint maxLoopDepth, uint maxIterations)
        {
            _maxLoopDepth = maxLoopDepth;
            _maxIterations = maxIterations;
        }

        public DiagnosticBag IntoDiagnostics() => _diagnostics;

        public void Expand(Document doc, Func<Expr, WclValue> evalExpr)
        {
            doc.Items = ExpandDocItems(doc.Items, evalExpr, 0);
        }

        private List<DocItem> ExpandDocItems(List<DocItem> items, Func<Expr, WclValue> evalExpr, uint depth)
        {
            var result = new List<DocItem>();
            foreach (var item in items)
            {
                if (item is BodyDocItem bdi)
                {
                    var expanded = ExpandBodyItems(new List<BodyItem> { bdi.BodyItem }, evalExpr, depth);
                    foreach (var e in expanded)
                        result.Add(new BodyDocItem(e));
                }
                else
                {
                    result.Add(item);
                }
            }
            return result;
        }

        private List<BodyItem> ExpandBodyItems(List<BodyItem> items, Func<Expr, WclValue> evalExpr, uint depth)
        {
            var result = new List<BodyItem>();
            foreach (var item in items)
            {
                switch (item)
                {
                    case ForLoopItem fl:
                        result.AddRange(ExpandForLoop(fl.ForLoop, evalExpr, depth));
                        break;
                    case ConditionalItem ci:
                        result.AddRange(ExpandConditional(ci.Conditional, evalExpr, depth));
                        break;
                    case BlockItem bi:
                        bi.Block.Body = ExpandBodyItems(bi.Block.Body, evalExpr, depth);
                        result.Add(bi);
                        break;
                    default:
                        result.Add(item);
                        break;
                }
            }
            return result;
        }

        private List<BodyItem> ExpandForLoop(ForLoop loop, Func<Expr, WclValue> evalExpr, uint depth)
        {
            if (depth >= _maxLoopDepth)
            {
                _diagnostics.ErrorWithCode("E029",
                    $"for loop nesting depth exceeded (max {_maxLoopDepth})", loop.Span);
                return new List<BodyItem>();
            }

            WclValue iterableVal;
            try { iterableVal = evalExpr(loop.Iterable); }
            catch (Exception ex)
            {
                _diagnostics.ErrorWithCode("E025",
                    $"cannot evaluate for loop iterable: {ex.Message}", loop.Iterable.GetSpan());
                return new List<BodyItem>();
            }

            if (iterableVal.Kind != WclValueKind.List)
            {
                _diagnostics.ErrorWithCode("E025",
                    $"for loop iterable must be a list, got {iterableVal.TypeName}", loop.Iterable.GetSpan());
                return new List<BodyItem>();
            }

            var list = iterableVal.AsList();
            var result = new List<BodyItem>();

            for (int i = 0; i < list.Count; i++)
            {
                if (_totalIterations >= _maxIterations)
                {
                    _diagnostics.ErrorWithCode("E028",
                        $"total iterations exceeded (max {_maxIterations})", loop.Span);
                    break;
                }
                _totalIterations++;

                var substituted = SubstituteBodyItems(loop.Body, loop.Iterator.Name,
                    list[i], loop.Index?.Name, i);
                var expanded = ExpandBodyItems(substituted, evalExpr, depth + 1);
                result.AddRange(expanded);
            }
            return result;
        }

        private List<BodyItem> ExpandConditional(Conditional cond, Func<Expr, WclValue> evalExpr, uint depth)
        {
            WclValue condVal;
            try { condVal = evalExpr(cond.Condition); }
            catch (Exception ex)
            {
                _diagnostics.ErrorWithCode("E026",
                    $"cannot evaluate condition: {ex.Message}", cond.Condition.GetSpan());
                return new List<BodyItem>();
            }

            if (condVal.IsTruthy() == true)
                return ExpandBodyItems(cond.ThenBody, evalExpr, depth);

            if (cond.ElseBranch is ElseIfBranch elseIf)
                return ExpandConditional(elseIf.Conditional, evalExpr, depth);
            if (cond.ElseBranch is ElseBlock elseBlock)
                return ExpandBodyItems(elseBlock.Body, evalExpr, depth);

            return new List<BodyItem>();
        }

        private List<BodyItem> SubstituteBodyItems(List<BodyItem> items, string iterName,
            WclValue iterValue, string? indexName, int indexValue)
        {
            return items.Select(item => SubstituteBodyItem(item, iterName, iterValue, indexName, indexValue)).ToList();
        }

        private BodyItem SubstituteBodyItem(BodyItem item, string iterName,
            WclValue iterValue, string? indexName, int indexValue)
        {
            switch (item)
            {
                case AttributeItem ai:
                    return new AttributeItem(new Core.Ast.Attribute(
                        ai.Attribute.Decorators, ai.Attribute.Name,
                        SubstituteExpr(ai.Attribute.Value, iterName, iterValue, indexName, indexValue),
                        ai.Attribute.Trivia, ai.Attribute.Span));
                case BlockItem bi:
                {
                    var newBody = SubstituteBodyItems(bi.Block.Body, iterName, iterValue, indexName, indexValue);
                    var newLabels = bi.Block.Labels.Select(l => SubstituteStringLit(l, iterName, iterValue, indexName, indexValue)).ToList();
                    var newId = bi.Block.InlineId != null
                        ? SubstituteInlineId(bi.Block.InlineId, iterName, iterValue, indexName, indexValue)
                        : null;
                    return new BlockItem(new Block(bi.Block.Decorators, bi.Block.Partial, bi.Block.Kind,
                        newId, newLabels, newBody, bi.Block.Trivia, bi.Block.Span));
                }
                case LetBindingItem li:
                    return new LetBindingItem(new LetBinding(
                        li.LetBinding.Decorators, li.LetBinding.Name,
                        SubstituteExpr(li.LetBinding.Value, iterName, iterValue, indexName, indexValue),
                        li.LetBinding.Trivia, li.LetBinding.Span));
                case ForLoopItem fl:
                    return new ForLoopItem(new ForLoop(fl.ForLoop.Iterator, fl.ForLoop.Index,
                        SubstituteExpr(fl.ForLoop.Iterable, iterName, iterValue, indexName, indexValue),
                        SubstituteBodyItems(fl.ForLoop.Body, iterName, iterValue, indexName, indexValue),
                        fl.ForLoop.Trivia, fl.ForLoop.Span));
                case ConditionalItem ci:
                    return new ConditionalItem(SubstituteConditional(ci.Conditional, iterName, iterValue, indexName, indexValue));
                default:
                    return item;
            }
        }

        private Conditional SubstituteConditional(Conditional cond, string iterName,
            WclValue iterValue, string? indexName, int indexValue)
        {
            ElseBranch? elseBranch = null;
            if (cond.ElseBranch is ElseIfBranch eib)
                elseBranch = new ElseIfBranch(SubstituteConditional(eib.Conditional, iterName, iterValue, indexName, indexValue));
            else if (cond.ElseBranch is ElseBlock eb)
                elseBranch = new ElseBlock(SubstituteBodyItems(eb.Body, iterName, iterValue, indexName, indexValue), eb.Trivia, eb.Span);

            return new Conditional(
                SubstituteExpr(cond.Condition, iterName, iterValue, indexName, indexValue),
                SubstituteBodyItems(cond.ThenBody, iterName, iterValue, indexName, indexValue),
                elseBranch, cond.Trivia, cond.Span);
        }

        private InlineId SubstituteInlineId(InlineId id, string iterName, WclValue iterValue, string? indexName, int indexValue)
        {
            if (id is InterpolatedInlineId interp)
            {
                var newParts = interp.Parts.Select(p =>
                {
                    if (p is InterpolationPart ip)
                        return (StringPart)new InterpolationPart(SubstituteExpr(ip.Expr, iterName, iterValue, indexName, indexValue));
                    return p;
                }).ToList();
                return new InterpolatedInlineId(newParts);
            }
            return id;
        }

        private StringLit SubstituteStringLit(StringLit sl, string iterName, WclValue iterValue, string? indexName, int indexValue)
        {
            var newParts = sl.Parts.Select(p =>
            {
                if (p is InterpolationPart ip)
                    return (StringPart)new InterpolationPart(SubstituteExpr(ip.Expr, iterName, iterValue, indexName, indexValue));
                return p;
            }).ToList();
            return new StringLit(newParts, sl.Span);
        }

        private Expr SubstituteExpr(Expr expr, string iterName, WclValue iterValue,
            string? indexName, int indexValue)
        {
            switch (expr)
            {
                case IdentExpr ie:
                    if (ie.Ident.Name == iterName) return ValueToExpr(iterValue, ie.GetSpan());
                    if (indexName != null && ie.Ident.Name == indexName)
                        return new IntLitExpr(indexValue, ie.GetSpan());
                    return expr;
                case BinaryOpExpr be:
                    return new BinaryOpExpr(
                        SubstituteExpr(be.Left, iterName, iterValue, indexName, indexValue),
                        be.Op,
                        SubstituteExpr(be.Right, iterName, iterValue, indexName, indexValue),
                        be.Span);
                case UnaryOpExpr ue:
                    return new UnaryOpExpr(ue.Op,
                        SubstituteExpr(ue.Operand, iterName, iterValue, indexName, indexValue), ue.Span);
                case FnCallExpr fc:
                    return new FnCallExpr(
                        SubstituteExpr(fc.Callee, iterName, iterValue, indexName, indexValue),
                        fc.Args.Select(a => a switch
                        {
                            PositionalCallArg pa => (CallArg)new PositionalCallArg(SubstituteExpr(pa.Value, iterName, iterValue, indexName, indexValue)),
                            NamedCallArg na => new NamedCallArg(na.Name, SubstituteExpr(na.Value, iterName, iterValue, indexName, indexValue)),
                            _ => a,
                        }).ToList(), fc.Span);
                case MemberAccessExpr ma:
                    return new MemberAccessExpr(
                        SubstituteExpr(ma.Object, iterName, iterValue, indexName, indexValue),
                        ma.Member, ma.Span);
                case IndexAccessExpr ia:
                    return new IndexAccessExpr(
                        SubstituteExpr(ia.Object, iterName, iterValue, indexName, indexValue),
                        SubstituteExpr(ia.Index, iterName, iterValue, indexName, indexValue), ia.Span);
                case TernaryExpr te:
                    return new TernaryExpr(
                        SubstituteExpr(te.Condition, iterName, iterValue, indexName, indexValue),
                        SubstituteExpr(te.ThenExpr, iterName, iterValue, indexName, indexValue),
                        SubstituteExpr(te.ElseExpr, iterName, iterValue, indexName, indexValue), te.Span);
                case ListExpr le:
                    return new ListExpr(
                        le.Items.Select(i => SubstituteExpr(i, iterName, iterValue, indexName, indexValue)).ToList(), le.Span);
                case MapExpr me:
                    return new MapExpr(
                        me.Entries.Select(e => (e.Key, SubstituteExpr(e.Value, iterName, iterValue, indexName, indexValue))).ToList(), me.Span);
                case SetExpr se:
                    return new SetExpr(
                        se.Items.Select(i => SubstituteExpr(i, iterName, iterValue, indexName, indexValue)).ToList(), se.Span);
                case ParenExpr pe:
                    return new ParenExpr(
                        SubstituteExpr(pe.Inner, iterName, iterValue, indexName, indexValue), pe.Span);
                case LambdaExpr le:
                    // Don't substitute if lambda param shadows iterator
                    if (le.Params.Any(p => p.Name == iterName)) return expr;
                    return new LambdaExpr(le.Params,
                        SubstituteExpr(le.Body, iterName, iterValue, indexName, indexValue), le.Span);
                case BlockExprNode be:
                    return new BlockExprNode(
                        be.Lets.Select(l => new LetBinding(l.Decorators, l.Name,
                            SubstituteExpr(l.Value, iterName, iterValue, indexName, indexValue),
                            l.Trivia, l.Span)).ToList(),
                        SubstituteExpr(be.FinalExpr, iterName, iterValue, indexName, indexValue), be.Span);
                case StringLitExpr sle:
                {
                    var newParts = sle.StringLit.Parts.Select(p =>
                    {
                        if (p is InterpolationPart ip)
                            return (StringPart)new InterpolationPart(SubstituteExpr(ip.Expr, iterName, iterValue, indexName, indexValue));
                        return p;
                    }).ToList();
                    return new StringLitExpr(new StringLit(newParts, sle.StringLit.Span));
                }
                default:
                    return expr;
            }
        }

        public static Expr ValueToExpr(WclValue value, Span span)
        {
            switch (value.Kind)
            {
                case WclValueKind.Int: return new IntLitExpr(value.AsInt(), span);
                case WclValueKind.Float: return new FloatLitExpr(value.AsFloat(), span);
                case WclValueKind.Bool: return new BoolLitExpr(value.AsBool(), span);
                case WclValueKind.Null: return new NullLitExpr(span);
                case WclValueKind.String:
                    return new StringLitExpr(new StringLit(
                        new List<StringPart> { new LiteralPart(value.AsString()) }, span));
                case WclValueKind.Identifier:
                    return new IdentifierLitExpr(new IdentifierLit(value.AsIdentifier(), span));
                case WclValueKind.List:
                    return new ListExpr(
                        value.AsList().Select(v => ValueToExpr(v, span)).ToList(), span);
                case WclValueKind.Map:
                {
                    var entries = new List<(MapKey, Expr)>();
                    foreach (var kvp in value.AsMap())
                        entries.Add((new IdentMapKey(new Ident(kvp.Key, span)), ValueToExpr(kvp.Value, span)));
                    return new MapExpr(entries, span);
                }
                default: return new NullLitExpr(span);
            }
        }
    }
}
