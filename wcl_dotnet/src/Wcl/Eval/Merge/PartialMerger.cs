using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval.Merge
{
    public enum ConflictMode
    {
        Strict,
        LastWins
    }

    public class PartialMerger
    {
        private readonly ConflictMode _mode;
        private readonly DiagnosticBag _diagnostics = new DiagnosticBag();

        public PartialMerger(ConflictMode mode) { _mode = mode; }

        public DiagnosticBag IntoDiagnostics() => _diagnostics;

        public void Merge(Document doc)
        {
            var groups = new Dictionary<string, List<(int Index, Block Block)>>();
            var nonPartialKeys = new HashSet<string>();
            var nonPartials = new List<DocItem>();

            for (int i = 0; i < doc.Items.Count; i++)
            {
                var item = doc.Items[i];
                if (item is BodyDocItem bdi && bdi.BodyItem is BlockItem bi)
                {
                    var key = BlockKey(bi.Block);
                    if (bi.Block.Partial)
                    {
                        // E033: Check for mixed partial/non-partial
                        if (nonPartialKeys.Contains(key))
                        {
                            _diagnostics.ErrorWithCode("E033",
                                $"block '{key}' is declared as both partial and non-partial",
                                bi.Block.Span);
                        }

                        if (!groups.ContainsKey(key))
                            groups[key] = new List<(int, Block)>();
                        groups[key].Add((i, bi.Block));
                    }
                    else
                    {
                        nonPartialKeys.Add(key);
                        // E033: Check if already seen as partial
                        if (groups.ContainsKey(key))
                        {
                            _diagnostics.ErrorWithCode("E033",
                                $"block '{key}' is declared as both partial and non-partial",
                                bi.Block.Span);
                        }
                        nonPartials.Add(item);
                    }
                }
                else
                {
                    nonPartials.Add(item);
                }
            }

            // Merge each group
            foreach (var kvp in groups)
            {
                if (kvp.Value.Count == 1)
                {
                    kvp.Value[0].Block.Partial = false;
                    nonPartials.Add(new BodyDocItem(new BlockItem(kvp.Value[0].Block)));
                    continue;
                }

                // Sort by @merge_order decorator if present
                var sorted = kvp.Value.OrderBy(v =>
                {
                    var mergeOrder = v.Block.Decorators
                        .FirstOrDefault(d => d.Name.Name == "merge_order");
                    if (mergeOrder?.Args.Count > 0 &&
                        mergeOrder.Args[0] is PositionalDecoratorArg pa &&
                        pa.Value is IntLitExpr ile)
                        return ile.Value;
                    return (long)v.Index; // Default: document order
                }).ToList();

                var merged = sorted[0].Block;
                merged.Partial = false;

                for (int i = 1; i < sorted.Count; i++)
                    MergeInto(merged, sorted[i].Block);

                // Validate @partial_requires
                ValidatePartialRequires(merged);

                nonPartials.Add(new BodyDocItem(new BlockItem(merged)));
            }

            doc.Items = nonPartials;
        }

        private string BlockKey(Block block)
        {
            var id = block.InlineId switch
            {
                LiteralInlineId lit => lit.Lit.Value,
                _ => ""
            };
            return $"{block.Kind.Name}#{id}";
        }

        private void MergeInto(Block target, Block source)
        {
            var existingAttrs = new HashSet<string>(
                target.Body.OfType<AttributeItem>().Select(a => a.Attribute.Name.Name));

            foreach (var item in source.Body)
            {
                if (item is AttributeItem ai)
                {
                    if (existingAttrs.Contains(ai.Attribute.Name.Name))
                    {
                        if (_mode == ConflictMode.Strict)
                        {
                            _diagnostics.ErrorWithCode("E031",
                                $"conflicting attribute '{ai.Attribute.Name.Name}' in partial merge",
                                ai.Attribute.Span);
                            continue;
                        }
                        // LastWins - replace
                        for (int i = 0; i < target.Body.Count; i++)
                        {
                            if (target.Body[i] is AttributeItem existing &&
                                existing.Attribute.Name.Name == ai.Attribute.Name.Name)
                            {
                                target.Body[i] = item;
                                break;
                            }
                        }
                    }
                    else
                    {
                        target.Body.Add(item);
                        existingAttrs.Add(ai.Attribute.Name.Name);
                    }
                }
                else
                {
                    target.Body.Add(item);
                }
            }

            // Merge decorators (deduplicate by name)
            var existingDecNames = new HashSet<string>(target.Decorators.Select(d => d.Name.Name));
            foreach (var dec in source.Decorators)
            {
                if (!existingDecNames.Contains(dec.Name.Name))
                {
                    target.Decorators.Add(dec);
                    existingDecNames.Add(dec.Name.Name);
                }
            }
        }

        private void ValidatePartialRequires(Block merged)
        {
            foreach (var dec in merged.Decorators)
            {
                if (dec.Name.Name == "partial_requires")
                {
                    var existingAttrs = new HashSet<string>(
                        merged.Body.OfType<AttributeItem>().Select(a => a.Attribute.Name.Name));

                    foreach (var arg in dec.Args)
                    {
                        if (arg is PositionalDecoratorArg pa && pa.Value is StringLitExpr sle)
                        {
                            var required = sle.StringLit.Parts.Count == 1 && sle.StringLit.Parts[0] is LiteralPart lp
                                ? lp.Value : "";
                            if (!existingAttrs.Contains(required))
                            {
                                _diagnostics.Error(
                                    $"@partial_requires: merged block is missing required field '{required}'",
                                    merged.Span);
                            }
                        }
                    }
                }
            }
        }
    }
}
