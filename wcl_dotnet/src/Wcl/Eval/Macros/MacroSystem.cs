using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

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
                    // Simple substitution - clone body items
                    return new List<BodyItem>(fb.Items);
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

                // Check for attribute macros
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
                            // Replace or add attribute
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
                }
            }
        }
    }
}
