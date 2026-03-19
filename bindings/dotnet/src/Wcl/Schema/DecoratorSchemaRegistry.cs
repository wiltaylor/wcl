using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Schema
{
    public class ResolvedDecoratorSchema
    {
        public string Name { get; }
        public List<DecoratorTarget> Targets { get; }
        public List<ResolvedField> Fields { get; }

        public ResolvedDecoratorSchema(string name, List<DecoratorTarget> targets, List<ResolvedField> fields)
        {
            Name = name; Targets = targets; Fields = fields;
        }
    }

    public class DecoratorSchemaRegistry
    {
        private readonly Dictionary<string, ResolvedDecoratorSchema> _schemas;

        public DecoratorSchemaRegistry()
        {
            _schemas = new Dictionary<string, ResolvedDecoratorSchema>();
            RegisterBuiltins();
        }

        private void RegisterBuiltins()
        {
            var allTargets = new List<DecoratorTarget>
                { DecoratorTarget.Block, DecoratorTarget.Attribute, DecoratorTarget.Table, DecoratorTarget.Schema };

            Register("deprecated", allTargets, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy())) { Optional = true }
            });
            Register("description", allTargets, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy()))
            });
            Register("optional", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>());
            Register("sensitive", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>());
            Register("warning", new List<DecoratorTarget> { DecoratorTarget.Block }, new List<ResolvedField>());
            Register("closed", new List<DecoratorTarget> { DecoratorTarget.Schema }, new List<ResolvedField>());
            Register("min", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>
            {
                new ResolvedField("_0", new IntTypeExpr(Span.Dummy()))
            });
            Register("max", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>
            {
                new ResolvedField("_0", new IntTypeExpr(Span.Dummy()))
            });
            Register("pattern", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy()))
            });
            Register("one_of", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>());
            Register("ref", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy()))
            });
            Register("id_pattern", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy()))
            });
            Register("schema", new List<DecoratorTarget> { DecoratorTarget.Block }, new List<ResolvedField>
            {
                new ResolvedField("_0", new StringTypeExpr(Span.Dummy()))
            });
            Register("merge_order", new List<DecoratorTarget> { DecoratorTarget.Block }, new List<ResolvedField>
            {
                new ResolvedField("_0", new IntTypeExpr(Span.Dummy()))
            });
            Register("partial_requires", new List<DecoratorTarget> { DecoratorTarget.Block }, new List<ResolvedField>());
            Register("table_index", new List<DecoratorTarget> { DecoratorTarget.Attribute }, new List<ResolvedField>());
        }

        private void Register(string name, List<DecoratorTarget> targets, List<ResolvedField> fields)
        {
            _schemas[name] = new ResolvedDecoratorSchema(name, targets, fields);
        }

        public void Collect(Document doc, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is DecoratorSchemaBodyItem dsi)
                {
                    var ds = dsi.DecoratorSchema;
                    var name = GetStringLitValue(ds.Name);
                    var fields = ds.Fields.Select(f => new ResolvedField(f.Name.Name, f.TypeExpr)).ToList();
                    _schemas[name] = new ResolvedDecoratorSchema(name, ds.Target, fields);
                }
            }
        }

        public void ValidateAll(Document doc, DiagnosticBag diags)
        {
            ValidateDecoratorsInDoc(doc.Items, diags);
        }

        private void ValidateDecoratorsInDoc(List<DocItem> items, DiagnosticBag diags)
        {
            foreach (var item in items)
            {
                if (item is BodyDocItem bdi)
                    ValidateDecoratorsInBody(bdi.BodyItem, diags);
            }
        }

        private void ValidateDecoratorsInBody(BodyItem item, DiagnosticBag diags)
        {
            switch (item)
            {
                case BlockItem bi:
                    foreach (var dec in bi.Block.Decorators)
                        ValidateDecorator(dec, DecoratorTarget.Block, diags);
                    foreach (var child in bi.Block.Body)
                        ValidateDecoratorsInBody(child, diags);
                    break;
                case AttributeItem ai:
                    foreach (var dec in ai.Attribute.Decorators)
                        ValidateDecorator(dec, DecoratorTarget.Attribute, diags);
                    break;
                case TableItem ti:
                    foreach (var dec in ti.Table.Decorators)
                        ValidateDecorator(dec, DecoratorTarget.Table, diags);
                    break;
                case ValidationItem vi:
                    foreach (var dec in vi.Validation.Decorators)
                        ValidateDecorator(dec, DecoratorTarget.Block, diags);
                    break;
            }
        }

        private void ValidateDecorator(Decorator dec, DecoratorTarget target, DiagnosticBag diags)
        {
            if (!_schemas.TryGetValue(dec.Name.Name, out var schema))
            {
                diags.ErrorWithCode("E060",
                    $"unknown decorator @{dec.Name.Name}", dec.Span);
                return;
            }

            if (schema.Targets.Count > 0 && !schema.Targets.Contains(target))
            {
                diags.ErrorWithCode("E061",
                    $"decorator @{dec.Name.Name} cannot be applied to {target}", dec.Span);
            }
        }

        private static string GetStringLitValue(StringLit sl)
        {
            if (sl.Parts.Count == 1 && sl.Parts[0] is LiteralPart lp) return lp.Value;
            var sb = new System.Text.StringBuilder();
            foreach (var part in sl.Parts)
                if (part is LiteralPart lp2) sb.Append(lp2.Value);
            return sb.ToString();
        }
    }
}
