using System.Collections.Generic;
using System.Linq;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Integration
{
    public class GapTests
    {
        // Macro parameter substitution
        [Fact]
        public void MacroParamSubstitution()
        {
            var doc = TestHelpers.ParseDoc(@"
                macro set_port(p) {
                    port = p
                }
                set_port(8080)
            ");
            Assert.Equal(WclValue.NewInt(8080), doc.Values["port"]);
        }

        [Fact]
        public void MacroMultipleParams()
        {
            var doc = TestHelpers.ParseDoc(@"
                macro config(h, p) {
                    host = h
                    port = p
                }
                config(""localhost"", 3000)
            ");
            Assert.Equal(WclValue.NewString("localhost"), doc.Values["host"]);
            Assert.Equal(WclValue.NewInt(3000), doc.Values["port"]);
        }

        // Member access on blocks
        [Fact]
        public void BlockLabelsAccess()
        {
            var doc = TestHelpers.ParseDoc(@"
                service ""prod"" {
                    port = 8080
                }
            ");
            var blocks = doc.Blocks();
            Assert.NotEmpty(blocks);
            Assert.Contains("prod", blocks[0].Labels);
        }

        // E033: Mixed partial/non-partial
        [Fact]
        public void MixedPartialNonPartialE033()
        {
            var doc = TestHelpers.ParseDoc(@"
                partial server main { port = 80 }
                server main { host = ""localhost"" }
            ");
            var e033 = doc.Diagnostics.Where(d => d.Code == "E033").ToList();
            Assert.NotEmpty(e033);
        }

        // Merge order
        [Fact]
        public void PartialMergeOrder()
        {
            var doc = TestHelpers.ParseDoc(@"
                @merge_order(2)
                partial server main { port = 80 }
                @merge_order(1)
                partial server main { host = ""localhost"" }
            ");
            var servers = doc.BlocksOfType("server");
            Assert.Single(servers);
        }

        // Control flow substitution completeness
        [Fact]
        public void ForLoopMapSubstitution()
        {
            var doc = TestHelpers.ParseDoc(@"
                for item in [1, 2] {
                    entry { value = { x = item } }
                }
            ");
            Assert.False(doc.HasErrors());
            Assert.Equal(2, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void ForLoopStringInterpolation()
        {
            var doc = TestHelpers.ParseDoc(@"
                let names = [""alpha"", ""beta""]
                for name in names {
                    entry { label = ""svc-${name}"" }
                }
            ");
            Assert.Equal(2, doc.BlocksOfType("entry").Count);
        }

        // Import jail check
        [Fact]
        public void ImportDisallowsAbsolutePaths()
        {
            var doc = WclParser.Parse("import \"/etc/passwd\"", new ParseOptions { AllowImports = true });
            var e013 = doc.Diagnostics.Where(d => d.Code == "E013").ToList();
            Assert.NotEmpty(e013);
        }

        // Query engine improvements
        [Fact]
        public void QueryFloatComparison()
        {
            var doc = TestHelpers.ParseDoc("service { score = 3.5 }\nservice { score = 7.5 }");
            var result = doc.Query("service | .score > 5.0");
            Assert.Single(result.AsList());
        }

        [Fact]
        public void QueryRecursiveInChildren()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main {
                    backend api {
                        port = 3000
                    }
                    backend web {
                        port = 8080
                    }
                }
            ");
            var result = doc.Query("..backend");
            Assert.Equal(2, result.AsList().Count);
        }

        // Schema E074: pattern validation
        [Fact]
        public void SchemaPatternValidation()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    name: string @pattern(""^[a-z]+$"")
                }
                config { name = ""ABC123"" }
            ");
            var e074 = doc.Diagnostics.Where(d => d.Code == "E074").ToList();
            Assert.NotEmpty(e074);
        }

        [Fact]
        public void SchemaPatternValid()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    name: string @pattern(""^[a-z]+$"")
                }
                config { name = ""hello"" }
            ");
            var e074 = doc.Diagnostics.Where(d => d.Code == "E074").ToList();
            Assert.Empty(e074);
        }

        // Error codes
        [Fact]
        public void UndefinedVariableE052()
        {
            var doc = TestHelpers.ParseDoc("x = undefined_var");
            Assert.True(doc.HasErrors());
            Assert.Contains("undefined variable", doc.Diagnostics[0].Message);
        }

        [Fact]
        public void DivisionByZeroError()
        {
            var doc = TestHelpers.ParseDoc("x = 1 / 0");
            Assert.True(doc.HasErrors());
            Assert.Contains("division by zero", doc.Diagnostics[0].Message);
        }

        // Serde BlockRef handling
        [Fact]
        public void DeserializeBlockRefToDict()
        {
            var attrs = new Wcl.Core.OrderedMap<string, WclValue>();
            attrs["port"] = WclValue.NewInt(8080);
            var br = new BlockRef("service", "main", new List<string> { "prod" },
                attrs, new List<BlockRef>(), new List<DecoratorValue>(), Wcl.Core.Span.Dummy());
            var val = WclValue.NewBlockRef(br);

            // BlockRef gets auto-converted to map with id/labels/attributes
            var result = Wcl.Serde.WclDeserializer.FromValue<Dictionary<string, WclValue>>(val);
            Assert.Equal(WclValue.NewString("main"), result["id"]);
            Assert.Equal(WclValue.NewInt(8080), result["port"]);
        }
    }
}
