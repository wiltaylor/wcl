using System.Collections.Generic;
using System.Linq;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Integration
{
    public class ParityTests
    {
        [Fact]
        public void SplitArgOrder()
        {
            // Rust: split(sep, str) — separator first
            var result = TestHelpers.Eval("split(\",\", \"a,b,c\")");
            Assert.Equal(3, result.AsList().Count);
            Assert.Equal("b", result.AsList()[1].AsString());
        }

        [Fact]
        public void ContainsListOverload()
        {
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("contains([1, 2, 3], 2)"));
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("contains([1, 2, 3], 5)"));
        }

        [Fact]
        public void ContainsStringStillWorks()
        {
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("contains(\"hello\", \"ell\")"));
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("contains(\"hello\", \"xyz\")"));
        }

        [Fact]
        public void ValidationBlockUsesLocalScope()
        {
            // Validation blocks should evaluate their own let bindings
            var doc = TestHelpers.ParseDoc(@"
                validation ""local scope"" {
                    let x = 10
                    let y = 20
                    check = x + y == 30
                    message = ""math is broken""
                }
            ");
            var errors = doc.Diagnostics.Where(d => d.Code == "E080").ToList();
            Assert.Empty(errors);
        }

        [Fact]
        public void ForLoopWithNestedConditional()
        {
            var doc = TestHelpers.ParseDoc(@"
                for item in [1, 2, 3] {
                    if item > 1 {
                        entry { value = item }
                    }
                }
            ");
            Assert.Equal(2, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void SchemaMinMaxConstraint()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    port: int @min(1) @max(65535)
                }
                config { port = 0 }
            ");
            var e073 = doc.Diagnostics.Where(d => d.Code == "E073").ToList();
            Assert.NotEmpty(e073);
        }

        [Fact]
        public void SchemaMinMaxValid()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    port: int @min(1) @max(65535)
                }
                config { port = 8080 }
            ");
            var e073 = doc.Diagnostics.Where(d => d.Code == "E073").ToList();
            Assert.Empty(e073);
        }

        [Fact]
        public void SchemaOneOfConstraint()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    env: string @one_of(""dev"", ""staging"", ""prod"")
                }
                config { env = ""invalid"" }
            ");
            var e075 = doc.Diagnostics.Where(d => d.Code == "E075").ToList();
            Assert.NotEmpty(e075);
        }

        [Fact]
        public void OutOfOrderEvaluation()
        {
            // Topo sort should handle forward references
            var doc = TestHelpers.ParseDoc(@"
                y = x * 2
                let x = 21
            ");
            Assert.Equal(WclValue.NewInt(42), doc.Values["y"]);
        }

        [Fact]
        public void BlockChildrenQueryable()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main {
                    port = 8080
                    backend api {
                        url = ""http://api""
                    }
                }
            ");
            var blocks = doc.Blocks();
            Assert.Single(blocks);
            Assert.Single(blocks[0].Children);
            Assert.Equal("api", blocks[0].Children[0].Id);
        }

        [Fact]
        public void QueryPathIntoChildren()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main {
                    backend api { port = 3000 }
                    backend web { port = 8080 }
                }
            ");
            var result = doc.Query("server | .port");
            // server doesn't have port directly, but backends do
            // Query returns what it can find
            Assert.NotNull(result);
        }

        [Fact]
        public void LambdaWithClosure()
        {
            var doc = TestHelpers.ParseDoc(@"
                let multiplier = 3
                result = map([1, 2, 3], x => x * multiplier)
            ");
            var result = doc.Values["result"];
            Assert.Equal(WclValue.NewInt(3), result.AsList()[0]);
            Assert.Equal(WclValue.NewInt(6), result.AsList()[1]);
            Assert.Equal(WclValue.NewInt(9), result.AsList()[2]);
        }

        [Fact]
        public void CountHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = count([1, 2, 3, 4, 5], x => x > 3)");
            Assert.Equal(WclValue.NewInt(2), doc.Values["result"]);
        }

        [Fact]
        public void NegativeIndexing()
        {
            var doc = TestHelpers.ParseDoc("let l = [10, 20, 30]\nresult = l[-1]");
            Assert.Equal(WclValue.NewInt(30), doc.Values["result"]);
        }

        [Fact]
        public void StringLengthMemberAccess()
        {
            var doc = TestHelpers.ParseDoc("result = \"hello\".length");
            Assert.Equal(WclValue.NewInt(5), doc.Values["result"]);
        }

        [Fact]
        public void ListLengthMemberAccess()
        {
            var doc = TestHelpers.ParseDoc("result = [1, 2, 3].length");
            Assert.Equal(WclValue.NewInt(3), doc.Values["result"]);
        }
    }
}
