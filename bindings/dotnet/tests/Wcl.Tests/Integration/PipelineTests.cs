using System;
using System.Collections.Generic;
using System.Linq;
using Wcl.Eval;
using Wcl.Eval.Functions;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Integration
{
    public class PipelineTests
    {
        [Fact]
        public void ParseSimple()
        {
            var doc = TestHelpers.ParseDoc("config { port = 8080 }");
            Assert.False(doc.HasErrors());
        }

        [Fact]
        public void ParseWithLet()
        {
            var doc = TestHelpers.ParseDoc("let x = 42\nconfig { port = x }");
            Assert.False(doc.HasErrors());
        }

        [Fact]
        public void HasErrorsOnValidInput()
        {
            var doc = TestHelpers.ParseDoc("x = 42");
            Assert.False(doc.HasErrors());
        }

        [Fact]
        public void BlocksOfType()
        {
            var doc = TestHelpers.ParseDoc("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }");
            var servers = doc.BlocksOfType("server");
            Assert.Equal(2, servers.Count);
            var clients = doc.BlocksOfType("client");
            Assert.Single(clients);
        }

        [Fact]
        public void QuerySelectsBlocks()
        {
            var doc = TestHelpers.ParseDoc("service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }");
            Assert.False(doc.HasErrors());
            var result = doc.Query("service");
            Assert.Equal(WclValueKind.List, result.Kind);
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void QueryWithProjection()
        {
            var doc = TestHelpers.ParseDoc("service { port = 8080 }\nservice { port = 9090 }");
            Assert.False(doc.HasErrors());
            var result = doc.Query("service | .port");
            Assert.Equal(WclValue.NewList(new List<WclValue>
                { WclValue.NewInt(8080), WclValue.NewInt(9090) }), result);
        }

        [Fact]
        public void LetBindingInForLoop()
        {
            var doc = TestHelpers.ParseDoc(@"
                let items = [1, 2, 3]
                for item in items {
                    entry { value = item }
                }
            ");
            var entries = doc.BlocksOfType("entry");
            Assert.Equal(3, entries.Count);
        }

        [Fact]
        public void LetBindingListStringsInForLoop()
        {
            var doc = TestHelpers.ParseDoc(@"
                let regions = [""us"", ""eu"", ""ap""]
                for region in regions {
                    server { name = region }
                }
            ");
            var servers = doc.BlocksOfType("server");
            Assert.Equal(3, servers.Count);
        }

        [Fact]
        public void BlockRefHasDecorator()
        {
            var doc = TestHelpers.ParseDoc("@deprecated(\"use v2\")\nservice main { port = 8080 }");
            var blocks = doc.Blocks();
            Assert.Single(blocks);
            Assert.True(blocks[0].HasDecorator("deprecated"));
            Assert.False(blocks[0].HasDecorator("nonexistent"));
        }

        [Fact]
        public void BlockRefGetAttribute()
        {
            var doc = TestHelpers.ParseDoc("service { port = 8080\n host = \"localhost\" }");
            var blocks = doc.Blocks();
            Assert.Equal(WclValue.NewInt(8080), blocks[0].Get("port"));
            Assert.Equal(WclValue.NewString("localhost"), blocks[0].Get("host"));
            Assert.Null(blocks[0].Get("missing"));
        }

        [Fact]
        public void DocumentHasDecorator()
        {
            var doc = TestHelpers.ParseDoc("@deprecated(\"old\")\nservice { port = 80 }\nserver { port = 443 }");
            Assert.True(doc.HasDecorator("deprecated"));
            Assert.False(doc.HasDecorator("nonexistent"));
        }

        [Fact]
        public void BlocksOfTypeResolved()
        {
            var doc = TestHelpers.ParseDoc("service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }");
            var services = doc.BlocksOfTypeResolved("service");
            Assert.Equal(2, services.Count);
            Assert.Equal(WclValue.NewInt(8080), services[0].Get("port"));
            Assert.Equal(WclValue.NewInt(9090), services[1].Get("port"));
        }

        [Fact]
        public void CustomFunctionRegistration()
        {
            var opts = new ParseOptions();
            opts.Functions.Register("double", args =>
                WclValue.NewInt(args[0].AsInt() * 2));
            var doc = WclParser.Parse("result = double(21)", opts);
            Assert.False(doc.HasErrors());
            Assert.Equal(WclValue.NewInt(42), doc.Values["result"]);
        }

        [Fact]
        public void CustomFunctionInControlFlow()
        {
            var opts = new ParseOptions();
            opts.Functions.Register("make_list", args =>
                WclValue.NewList(new List<WclValue> { WclValue.NewInt(1), WclValue.NewInt(2) }));
            var doc = WclParser.Parse("for item in make_list() { entry { value = item } }", opts);
            Assert.False(doc.HasErrors());
            var entries = doc.BlocksOfType("entry");
            Assert.Equal(2, entries.Count);
        }

        [Fact]
        public void DeclaredButUnregisteredFunctionE053()
        {
            var doc = TestHelpers.ParseDoc(@"
                declare my_fn(input: string) -> string
                result = my_fn(""hello"")
            ");
            var e053 = doc.Diagnostics.Where(d => d.Code == "E053").ToList();
            Assert.NotEmpty(e053);
        }

        [Fact]
        public void DeclaredAndRegisteredFunctionWorks()
        {
            var opts = new ParseOptions();
            opts.Functions.Register("my_fn", args =>
                WclValue.NewString($"processed: {args[0].AsString()}"));
            var doc = WclParser.Parse(@"
                declare my_fn(input: string) -> string
                result = my_fn(""hello"")
            ", opts);
            Assert.False(doc.HasErrors());
            Assert.Equal(WclValue.NewString("processed: hello"), doc.Values["result"]);
        }

        [Fact]
        public void FunctionRegistryWithSignature()
        {
            var reg = new FunctionRegistry();
            reg.Register("greet", args => WclValue.NewString($"Hello, {args[0].AsString()}!"),
                new FunctionSignature("greet", new List<string> { "name: string" }, "string", "Greet someone"));
            Assert.Single(reg.Functions);
            Assert.Single(reg.Signatures);
            Assert.Equal("greet", reg.Signatures[0].Name);
        }

        [Fact]
        public void BuiltinSignaturesComplete()
        {
            var sigs = BuiltinRegistry.BuiltinSignatures();
            Assert.True(sigs.Count >= 50, $"expected >= 50, got {sigs.Count}");
            Assert.Contains(sigs, s => s.Name == "upper");
            Assert.Contains(sigs, s => s.Name == "len");
            Assert.Contains(sigs, s => s.Name == "sha256");
        }

        [Fact]
        public void FunctionStubToWcl()
        {
            var stub = new Library.FunctionStub("my_fn",
                new List<(string, string)> { ("input", "string"), ("count", "int") },
                "string", "Transform input");
            Assert.Equal("declare my_fn(input: string, count: int) -> string\n", stub.ToWcl());
        }

        [Fact]
        public void LibraryBuilderBuild()
        {
            var builder = new Library.LibraryBuilder("myapp");
            builder.AddSchemaText("schema \"config\" {\n    port: int\n}\n");
            builder.AddFunctionStub(new Library.FunctionStub("greet",
                new List<(string, string)> { ("name", "string") }, "string"));
            var content = builder.Build();
            Assert.Contains("schema \"config\"", content);
            Assert.Contains("declare greet(name: string) -> string", content);
        }

        [Fact]
        public void PartialBlockMerge()
        {
            var doc = TestHelpers.ParseDoc(@"
                partial server main { port = 80 }
                partial server main { host = ""localhost"" }
            ");
            var servers = doc.BlocksOfType("server");
            Assert.Single(servers);
        }

        [Fact]
        public void ConditionalExpansion()
        {
            var doc = TestHelpers.ParseDoc(@"
                if true {
                    x = 1
                }
            ");
            Assert.False(doc.HasErrors());
            Assert.True(doc.Values.ContainsKey("x"));
        }

        [Fact]
        public void ConditionalElse()
        {
            var doc = TestHelpers.ParseDoc(@"
                if false {
                    x = 1
                } else {
                    x = 2
                }
            ");
            Assert.False(doc.HasErrors());
            Assert.Equal(WclValue.NewInt(2), doc.Values["x"]);
        }

        [Fact]
        public void NestedBlocks()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main {
                    port = 8080
                    backend api {
                        url = ""http://api""
                    }
                }
            ");
            Assert.False(doc.HasErrors());
        }

        [Fact]
        public void FromString()
        {
            // Using Dictionary for simple deserialization
            var result = WclParser.FromString<Dictionary<string, long>>("port = 8080\ntimeout = 30");
            Assert.Equal(8080L, result["port"]);
            Assert.Equal(30L, result["timeout"]);
        }
    }
}
