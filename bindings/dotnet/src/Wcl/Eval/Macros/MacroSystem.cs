using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval.ControlFlow;

namespace Wcl.Eval.Macros
{
    public class MacroRegistry
    {
        private readonly Dictionary<string, MacroDef> _functionMacros = new Dictionary<string, MacroDef>();
        private readonly Dictionary<string, MacroDef> _attributeMacros = new Dictionary<string, MacroDef>();

        public void Collect(Document doc, DiagnosticBag diags)
        {
            var remaining = new List<DocItem>();
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is MacroDefItem mdi)
                {
                    var def = mdi.MacroDef;
                    var dict = def.Kind == MacroKind.Function ? _functionMacros : _attributeMacros;
                    if (dict.ContainsKey(def.Name.Name))
                    {
                        diags.ErrorWithCode("E016",
                            $"duplicate macro name: '{def.Name.Name}'", def.Span);
                    }
                    dict[def.Name.Name] = def;
                }
                else
                {
                    remaining.Add(item);
                }
            }
            doc.Items = remaining;
        }

        public MacroDef? GetFunction(string name) =>
            _functionMacros.TryGetValue(name, out var m) ? m : null;

        public MacroDef? GetAttribute(string name) =>
            _attributeMacros.TryGetValue(name, out var m) ? m : null;
    }

    public class MacroExpander
    {
        private readonly MacroRegistry _registry;
        private readonly uint _maxDepth;
        private readonly DiagnosticBag _diagnostics = new DiagnosticBag();

        public MacroExpander(MacroRegistry registry, uint maxDepth)
        {
            _registry = registry;
            _maxDepth = maxDepth;
        }

        public DiagnosticBag IntoDiagnostics() => _diagnostics;

        public void Expand(Document doc)
        {
            for (uint pass = 0; pass < _maxDepth; pass++)
            {
                bool changed = false;
                doc.Items = ExpandDocItems(doc.Items, ref changed);
                if (!changed) break;
                if (pass == _maxDepth - 1)
                {
                    _diagnostics.ErrorWithCode("E022",
                        $"macro expansion did not converge after {_maxDepth} passes",
                        Span.Dummy());
                }
            }
        }

        private List<DocItem> ExpandDocItems(List<DocItem> items, ref bool changed)
        {
            var result = new List<DocItem>();
            foreach (var item in items)
            {
                if (item is BodyDocItem bdi)
                {
                    var expanded = ExpandBodyItem(bdi.BodyItem, ref changed);
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

        private List<BodyItem> ExpandBodyItem(BodyItem item, ref bool changed)
        {
            if (item is MacroCallItem mc)
            {
                var def = _registry.GetFunction(mc.MacroCall.Name.Name);
                if (def != null && def.Body is FunctionMacroBody fb)
                {
                    changed = true;

                    // Build parameter bindings: param name -> arg expression value
                    var bindings = new Dictionary<string, Expr>();
                    for (int i = 0; i < def.Params.Count; i++)
                    {
                        var paramName = def.Params[i].Name.Name;
                        if (i < mc.MacroCall.Args.Count)
                        {
                            var arg = mc.MacroCall.Args[i];
                            switch (arg)
                            {
                                case PositionalMacroArg pa:
                                    bindings[paramName] = pa.Value;
                                    break;
                                case NamedMacroArg na:
                                    bindings[na.Name.Name] = na.Value;
                                    break;
                            }
                        }
                        else if (def.Params[i].Default != null)
                        {
                            bindings[paramName] = def.Params[i].Default!;
                        }
                    }

                    // Also handle named args that aren't positionally matched
                    foreach (var arg in mc.MacroCall.Args)
                    {
                        if (arg is NamedMacroArg na && !bindings.ContainsKey(na.Name.Name))
                            bindings[na.Name.Name] = na.Value;
                    }

                    // Substitute parameters in body items
                    var result = new List<BodyItem>();
                    foreach (var bodyItem in fb.Items)
                    {
                        result.Add(SubstituteParams(bodyItem, bindings));
                    }
                    return result;
                }
                else if (def == null)
                {
                    // Not a known macro - leave as-is (might be a regular function call parsed as macro call)
                    return new List<BodyItem> { item };
                }
            }

            // Recurse into blocks
            if (item is BlockItem bi)
            {
                var newBody = new List<BodyItem>();
                foreach (var child in bi.Block.Body)
                {
                    var expanded = ExpandBodyItem(child, ref changed);
                    newBody.AddRange(expanded);
                }
                bi.Block.Body = newBody;

                // Apply attribute macros
                var remainingDecorators = new List<Decorator>();
                foreach (var dec in bi.Block.Decorators)
                {
                    var attrMacro = _registry.GetAttribute(dec.Name.Name);
                    if (attrMacro != null && attrMacro.Body is AttributeMacroBody ab)
                    {
                        changed = true;
                        ApplyTransformDirectives(bi.Block, ab.Directives);
                    }
                    else
                    {
                        remainingDecorators.Add(dec);
                    }
                }
                bi.Block.Decorators = remainingDecorators;
            }

            return new List<BodyItem> { item };
        }

        private BodyItem SubstituteParams(BodyItem item, Dictionary<string, Expr> bindings)
        {
            switch (item)
            {
                case AttributeItem ai:
                    return new AttributeItem(new Attribute(
                        ai.Attribute.Decorators, ai.Attribute.Name,
                        SubstituteExprParams(ai.Attribute.Value, bindings),
                        ai.Attribute.Trivia, ai.Attribute.Span));
                case LetBindingItem li:
                    return new LetBindingItem(new LetBinding(
                        li.LetBinding.Decorators, li.LetBinding.Name,
                        SubstituteExprParams(li.LetBinding.Value, bindings),
                        li.LetBinding.Trivia, li.LetBinding.Span));
                case BlockItem bi:
                {
                    var newBody = bi.Block.Body.Select(b => SubstituteParams(b, bindings)).ToList();
                    return new BlockItem(new Block(bi.Block.Decorators, bi.Block.Partial,
                        bi.Block.Kind, bi.Block.InlineId, bi.Block.Labels, newBody,
                        bi.Block.Trivia, bi.Block.Span));
                }
                default:
                    return item;
            }
        }

        private Expr SubstituteExprParams(Expr expr, Dictionary<string, Expr> bindings)
        {
            switch (expr)
            {
                case IdentExpr ie:
                    if (bindings.TryGetValue(ie.Ident.Name, out var replacement))
                        return replacement;
                    return expr;
                case BinaryOpExpr be:
                    return new BinaryOpExpr(
                        SubstituteExprParams(be.Left, bindings),
                        be.Op,
                        SubstituteExprParams(be.Right, bindings),
                        be.Span);
                case UnaryOpExpr ue:
                    return new UnaryOpExpr(ue.Op,
                        SubstituteExprParams(ue.Operand, bindings), ue.Span);
                case FnCallExpr fc:
                    return new FnCallExpr(
                        SubstituteExprParams(fc.Callee, bindings),
                        fc.Args.Select(a => a switch
                        {
                            PositionalCallArg pa => (CallArg)new PositionalCallArg(SubstituteExprParams(pa.Value, bindings)),
                            NamedCallArg na => new NamedCallArg(na.Name, SubstituteExprParams(na.Value, bindings)),
                            _ => a,
                        }).ToList(),
                        fc.Span);
                case MemberAccessExpr ma:
                    return new MemberAccessExpr(SubstituteExprParams(ma.Object, bindings), ma.Member, ma.Span);
                case IndexAccessExpr ia:
                    return new IndexAccessExpr(SubstituteExprParams(ia.Object, bindings),
                        SubstituteExprParams(ia.Index, bindings), ia.Span);
                case TernaryExpr te:
                    return new TernaryExpr(
                        SubstituteExprParams(te.Condition, bindings),
                        SubstituteExprParams(te.ThenExpr, bindings),
                        SubstituteExprParams(te.ElseExpr, bindings), te.Span);
                case ListExpr le:
                    return new ListExpr(le.Items.Select(i => SubstituteExprParams(i, bindings)).ToList(), le.Span);
                case MapExpr me:
                    return new MapExpr(me.Entries.Select(e => (e.Key, SubstituteExprParams(e.Value, bindings))).ToList(), me.Span);
                case ParenExpr pe:
                    return new ParenExpr(SubstituteExprParams(pe.Inner, bindings), pe.Span);
                case LambdaExpr le:
                    // Don't substitute params that are shadowed by lambda params
                    var filtered = new Dictionary<string, Expr>(bindings);
                    foreach (var p in le.Params) filtered.Remove(p.Name);
                    return new LambdaExpr(le.Params, SubstituteExprParams(le.Body, filtered), le.Span);
                default:
                    return expr;
            }
        }

        private void ApplyTransformDirectives(Block block, List<TransformDirective> directives)
        {
            foreach (var directive in directives)
            {
                switch (directive)
                {
                    case InjectDirective inj:
                        block.Body.AddRange(inj.Body);
                        break;
                    case SetDirective set:
                        foreach (var attr in set.Attrs)
                        {
                            bool found = false;
                            for (int i = 0; i < block.Body.Count; i++)
                            {
                                if (block.Body[i] is AttributeItem existing &&
                                    existing.Attribute.Name.Name == attr.Name.Name)
                                {
                                    block.Body[i] = new AttributeItem(attr);
                                    found = true;
                                    break;
                                }
                            }
                            if (!found) block.Body.Add(new AttributeItem(attr));
                        }
                        break;
                    case RemoveDirective rem:
                        var namesToRemove = new HashSet<string>(rem.Names.Select(n => n.Name));
                        block.Body.RemoveAll(b => b is AttributeItem ai && namesToRemove.Contains(ai.Attribute.Name.Name));
                        break;
                    case WhenDirective wd:
                        // When directives are conditionally applied (simplified: always apply for now)
                        ApplyTransformDirectives(block, wd.Directives);
                        break;
                }
            }
        }
    }
}
