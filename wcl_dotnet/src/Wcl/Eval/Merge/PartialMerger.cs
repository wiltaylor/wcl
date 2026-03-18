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
            var nonPartials = new List<DocItem>();

            for (int i = 0; i < doc.Items.Count; i++)
            {
                var item = doc.Items[i];
                if (item is BodyDocItem bdi && bdi.BodyItem is BlockItem bi && bi.Block.Partial)
                {
                    var key = BlockKey(bi.Block);
                    if (!groups.ContainsKey(key))
                        groups[key] = new List<(int, Block)>();
                    groups[key].Add((i, bi.Block));
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
                    // Single partial - just remove partial flag
                    kvp.Value[0].Block.Partial = false;
                    nonPartials.Add(new BodyDocItem(new BlockItem(kvp.Value[0].Block)));
                    continue;
                }

                var merged = kvp.Value[0].Block;
                merged.Partial = false;

                for (int i = 1; i < kvp.Value.Count; i++)
                {
                    MergeInto(merged, kvp.Value[i].Block);
                }

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
                            _diagnostics.Error(
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

            // Merge decorators
            target.Decorators.AddRange(source.Decorators);
        }
    }
}
