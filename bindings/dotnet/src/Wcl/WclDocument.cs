using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;
using Wcl.Eval.Query;
using Wcl.Schema;

namespace Wcl
{
    public class WclDocument
    {
        public Document Ast { get; }
        public OrderedMap<string, WclValue> Values { get; }
        public List<Diagnostic> Diagnostics { get; }
        public SourceMap SourceMap { get; }
        public SchemaRegistry Schemas { get; }
        public DecoratorSchemaRegistry DecoratorSchemas { get; }

        public WclDocument(Document ast, OrderedMap<string, WclValue> values,
                           List<Diagnostic> diagnostics, SourceMap sourceMap,
                           SchemaRegistry schemas, DecoratorSchemaRegistry decoratorSchemas)
        {
            Ast = ast; Values = values; Diagnostics = diagnostics;
            SourceMap = sourceMap; Schemas = schemas; DecoratorSchemas = decoratorSchemas;
        }

        public List<Block> BlocksOfType(string kind) =>
            Ast.Items
                .OfType<BodyDocItem>()
                .Select(b => b.BodyItem)
                .OfType<BlockItem>()
                .Where(bi => bi.Block.Kind.Name == kind)
                .Select(bi => bi.Block)
                .ToList();

        public List<BlockRef> Blocks()
        {
            var evaluator = new Evaluator();
            var scope = evaluator.Scopes.CreateScope(ScopeKind.Module, null);
            return Ast.Items
                .OfType<BodyDocItem>()
                .Select(b => b.BodyItem)
                .OfType<BlockItem>()
                .Select(bi => BlockToRef(bi.Block, evaluator, scope))
                .ToList();
        }

        public List<BlockRef> BlocksOfTypeResolved(string kind) =>
            Blocks().Where(b => b.Kind == kind).ToList();

        public WclValue Query(string queryStr)
        {
            var fileId = new FileId(9999);
            var pipeline = Core.Parser.WclParser.ParseQuery(queryStr, fileId);
            if (pipeline == null)
                throw new System.Exception("query parse error");

            var blocks = Blocks();
            var evaluator = new Evaluator();
            var scope = evaluator.Scopes.CreateScope(ScopeKind.Module, null);
            var engine = new QueryEngine();
            return engine.Execute(pipeline, blocks, evaluator, scope);
        }

        public bool HasDecorator(string decoratorName) =>
            Blocks().Any(b => b.HasDecorator(decoratorName));

        public bool HasErrors() => Diagnostics.Any(d => d.IsError);

        public List<Diagnostic> Errors() => Diagnostics.Where(d => d.IsError).ToList();

        private static BlockRef BlockToRef(Block block, Evaluator evaluator, ScopeId scope)
        {
            var kind = block.Kind.Name;
            string? id = null;
            if (block.InlineId is LiteralInlineId lit) id = lit.Lit.Value;

            var labels = block.Labels
                .Where(sl => sl.Parts.Count == 1 && sl.Parts[0] is LiteralPart)
                .Select(sl => ((LiteralPart)sl.Parts[0]).Value)
                .ToList();

            var attrs = new OrderedMap<string, WclValue>();
            foreach (var bodyItem in block.Body)
            {
                if (bodyItem is AttributeItem ai)
                {
                    try
                    {
                        var val = evaluator.EvalExpr(ai.Attribute.Value, scope);
                        attrs[ai.Attribute.Name.Name] = val;
                    }
                    catch { }
                }
            }

            var children = block.Body
                .OfType<BlockItem>()
                .Select(bi => BlockToRef(bi.Block, evaluator, scope))
                .ToList();

            var decorators = block.Decorators.Select(d =>
            {
                var args = new OrderedMap<string, WclValue>();
                foreach (var arg in d.Args)
                {
                    if (arg is NamedDecoratorArg na)
                    {
                        try { args[na.Name.Name] = evaluator.EvalExpr(na.Value, scope); }
                        catch { }
                    }
                    else if (arg is PositionalDecoratorArg pa)
                    {
                        try { args[$"_{args.Count}"] = evaluator.EvalExpr(pa.Value, scope); }
                        catch { }
                    }
                }
                return new DecoratorValue(d.Name.Name, args);
            }).ToList();

            return new BlockRef(kind, id, labels, attrs, children, decorators, block.Span);
        }
    }
}
