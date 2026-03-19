using System.Collections.Generic;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Schema
{
    public class IdRegistry
    {
        private readonly Dictionary<string, Span> _ids = new Dictionary<string, Span>();

        public void CheckDocument(Document doc, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is BlockItem bi)
                    CheckBlock(bi.Block, diags);
            }
        }

        private void CheckBlock(Block block, DiagnosticBag diags)
        {
            if (block.Partial) return; // Partials are allowed duplicates before merge

            if (block.InlineId != null)
            {
                var id = GetInlineIdValue(block.InlineId);
                if (id != null)
                {
                    var key = $"{block.Kind.Name}#{id}";
                    if (_ids.TryGetValue(key, out var existing))
                    {
                        diags.ErrorWithCode("E030",
                            $"duplicate block ID: {block.Kind.Name} {id}",
                            block.Span)
                            .WithLabel(existing, "first defined here");
                    }
                    else
                    {
                        _ids[key] = block.Span;
                    }
                }
            }

            foreach (var bodyItem in block.Body)
            {
                if (bodyItem is BlockItem child)
                    CheckBlock(child.Block, diags);
            }
        }

        private static string? GetInlineIdValue(InlineId inlineId)
        {
            if (inlineId is LiteralInlineId lit) return lit.Lit.Value;
            return null;
        }
    }
}
