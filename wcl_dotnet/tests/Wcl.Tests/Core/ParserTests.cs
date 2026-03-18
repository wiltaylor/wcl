using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Core.Parser;
using Xunit;

namespace Wcl.Tests.Core
{
    public class ParserTests
    {
        private static (Document Doc, DiagnosticBag Diags) Parse(string source) =>
            Wcl.Core.Parser.WclParser.Parse(source, new FileId(0));

        [Fact]
        public void ParseSimpleBlock()
        {
            var (doc, diags) = Parse("config { port = 8080 }");
            Assert.False(diags.HasErrors);
            Assert.Single(doc.Items);
            var block = ((BodyDocItem)doc.Items[0]).BodyItem as BlockItem;
            Assert.NotNull(block);
            Assert.Equal("config", block!.Block.Kind.Name);
        }

        [Fact]
        public void ParseLetBinding()
        {
            var (doc, diags) = Parse("let x = 42");
            Assert.False(diags.HasErrors);
            var let = ((BodyDocItem)doc.Items[0]).BodyItem as LetBindingItem;
            Assert.NotNull(let);
            Assert.Equal("x", let!.LetBinding.Name.Name);
        }

        [Fact]
        public void ParseAttribute()
        {
            var (doc, _) = Parse("name = \"hello\"");
            var attr = ((BodyDocItem)doc.Items[0]).BodyItem as AttributeItem;
            Assert.NotNull(attr);
            Assert.Equal("name", attr!.Attribute.Name.Name);
        }

        [Fact]
        public void ParseBlockWithInlineId()
        {
            var (doc, diags) = Parse("server main { port = 80 }");
            Assert.False(diags.HasErrors);
            var block = ((BodyDocItem)doc.Items[0]).BodyItem as BlockItem;
            Assert.NotNull(block!.Block.InlineId);
        }

        [Fact]
        public void ParseDecorator()
        {
            var (doc, diags) = Parse("@deprecated(\"use v2\")\nserver { port = 80 }");
            Assert.False(diags.HasErrors);
            var block = ((BodyDocItem)doc.Items[0]).BodyItem as BlockItem;
            Assert.Single(block!.Block.Decorators);
            Assert.Equal("deprecated", block.Block.Decorators[0].Name.Name);
        }

        [Fact]
        public void ParseForLoop()
        {
            var (doc, diags) = Parse("for item in [1, 2, 3] { entry { value = item } }");
            Assert.False(diags.HasErrors);
            var fl = ((BodyDocItem)doc.Items[0]).BodyItem as ForLoopItem;
            Assert.NotNull(fl);
            Assert.Equal("item", fl!.ForLoop.Iterator.Name);
        }

        [Fact]
        public void ParseConditional()
        {
            var (doc, diags) = Parse("if true { x = 1 } else { x = 2 }");
            Assert.False(diags.HasErrors);
            var cond = ((BodyDocItem)doc.Items[0]).BodyItem as ConditionalItem;
            Assert.NotNull(cond);
        }

        [Fact]
        public void ParseSchema()
        {
            var (doc, diags) = Parse("schema \"config\" { port: int\n host: string }");
            Assert.False(diags.HasErrors);
            var schema = ((BodyDocItem)doc.Items[0]).BodyItem as SchemaItem;
            Assert.NotNull(schema);
            Assert.Equal(2, schema!.Schema.Fields.Count);
        }

        [Fact]
        public void ParseImportRelative()
        {
            var (doc, diags) = Parse("import \"./other.wcl\"");
            Assert.False(diags.HasErrors);
            var imp = doc.Items[0] as ImportItem;
            Assert.NotNull(imp);
            Assert.Equal(ImportKind.Relative, imp!.Import.Kind);
        }

        [Fact]
        public void ParseImportLibrary()
        {
            var (doc, diags) = Parse("import <stdlib.wcl>");
            Assert.False(diags.HasErrors);
            var imp = doc.Items[0] as ImportItem;
            Assert.NotNull(imp);
            Assert.Equal(ImportKind.Library, imp!.Import.Kind);
        }

        [Fact]
        public void ParseFunctionDecl()
        {
            var (doc, diags) = Parse("declare my_fn(input: string, count: int) -> string");
            Assert.False(diags.HasErrors);
            var fd = doc.Items[0] as FunctionDeclItem;
            Assert.NotNull(fd);
            Assert.Equal("my_fn", fd!.FunctionDecl.Name.Name);
            Assert.Equal(2, fd.FunctionDecl.Params.Count);
        }

        [Fact]
        public void ParseTable()
        {
            var (doc, diags) = Parse("table users {\n  name: string\n  age: int\n  | \"Alice\" | 30 |\n}");
            Assert.False(diags.HasErrors);
            var table = ((BodyDocItem)doc.Items[0]).BodyItem as TableItem;
            Assert.NotNull(table);
            Assert.Equal(2, table!.Table.Columns.Count);
            Assert.Single(table.Table.Rows);
        }

        [Fact]
        public void ParseExpressionPrecedence()
        {
            var (doc, _) = Parse("result = 1 + 2 * 3");
            var attr = ((BodyDocItem)doc.Items[0]).BodyItem as AttributeItem;
            Assert.NotNull(attr);
            // Should be: 1 + (2 * 3) due to precedence
            var binOp = attr!.Attribute.Value as BinaryOpExpr;
            Assert.NotNull(binOp);
            Assert.Equal(BinOp.Add, binOp!.Op);
        }

        [Fact]
        public void ParseLambda()
        {
            var (doc, _) = Parse("f = (x, y) => x + y");
            var attr = ((BodyDocItem)doc.Items[0]).BodyItem as AttributeItem;
            var lambda = attr!.Attribute.Value as LambdaExpr;
            Assert.NotNull(lambda);
            Assert.Equal(2, lambda!.Params.Count);
        }

        [Fact]
        public void ParseTernary()
        {
            var (doc, _) = Parse("result = true ? 1 : 2");
            var attr = ((BodyDocItem)doc.Items[0]).BodyItem as AttributeItem;
            var ternary = attr!.Attribute.Value as TernaryExpr;
            Assert.NotNull(ternary);
        }

        [Fact]
        public void ParseValidation()
        {
            var (doc, diags) = Parse("validation \"check\" { let x = 10\n check = x > 0\n message = \"failed\" }");
            Assert.False(diags.HasErrors);
            var val = ((BodyDocItem)doc.Items[0]).BodyItem as ValidationItem;
            Assert.NotNull(val);
            Assert.Single(val!.Validation.Lets);
        }

        [Fact]
        public void ParseDecoratorSchema()
        {
            var (doc, diags) = Parse("decorator_schema \"custom\" { target = [block, attribute]\n level: int }");
            Assert.False(diags.HasErrors);
        }

        [Fact]
        public void ParseQueryExpr()
        {
            var (doc, diags) = Parse("result = query(service | .port > 8000)");
            Assert.False(diags.HasErrors);
        }

        [Fact]
        public void ParseRefExpr()
        {
            var (doc, _) = Parse("result = ref(my-service)");
            var attr = ((BodyDocItem)doc.Items[0]).BodyItem as AttributeItem;
            var refExpr = attr!.Attribute.Value as RefExpr;
            Assert.NotNull(refExpr);
            Assert.Equal("my-service", refExpr!.Id.Value);
        }
    }
}
