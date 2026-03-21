using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Wcl;
using Wcl.Eval;
using Wcl.Library;
using Xunit;

namespace Wcl.Tests
{
    public class WclTests
    {
        [Fact]
        public void ParseSimpleKeyValue()
        {
            using var doc = WclParser.Parse("x = 42\ny = \"hello\"");
            Assert.False(doc.HasErrors());

            var values = doc.Values;
            Assert.Equal(42L, values["x"].AsInt());
            Assert.Equal("hello", values["y"].AsString());
        }

        [Fact]
        public void ParseWithErrors()
        {
            using var doc = WclParser.Parse("x = @invalid");
            Assert.True(doc.HasErrors());

            var errors = doc.Errors();
            Assert.NotEmpty(errors);
            Assert.Equal("error", errors[0].Severity);
        }

        [Fact]
        public void ParseFile()
        {
            var dir = Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString());
            Directory.CreateDirectory(dir);
            var path = Path.Combine(dir, "test.wcl");
            File.WriteAllText(path, "port = 8080\nhost = \"localhost\"");

            try
            {
                using var doc = WclParser.ParseFile(path);
                Assert.False(doc.HasErrors());
                Assert.Equal(8080L, doc.Values["port"].AsInt());
                Assert.Equal("localhost", doc.Values["host"].AsString());
            }
            finally
            {
                Directory.Delete(dir, true);
            }
        }

        [Fact]
        public void ParseFileNotFound()
        {
            Assert.ThrowsAny<Exception>(() => WclParser.ParseFile("/nonexistent/path.wcl"));
        }

        [Fact]
        public void QueryExecution()
        {
            using var doc = WclParser.Parse("service { port = 8080 }\nservice { port = 9090 }");
            Assert.False(doc.HasErrors());

            var result = doc.Query("service | .port");
            var ports = result.AsList();
            Assert.Equal(2, ports.Count);
            Assert.Equal(8080L, ports[0].AsInt());
            Assert.Equal(9090L, ports[1].AsInt());
        }

        [Fact]
        public void CustomFunctions()
        {
            var options = new ParseOptions
            {
                Functions = new Dictionary<string, Func<WclValue[], WclValue>>
                {
                    ["double"] = args =>
                    {
                        var n = args[0].AsInt();
                        return WclValue.NewInt(n * 2);
                    }
                }
            };

            using var doc = WclParser.Parse("result = double(21)", options);
            Assert.False(doc.HasErrors());
            Assert.Equal(42L, doc.Values["result"].AsInt());
        }

        [Fact]
        public void BlocksAndBlocksOfType()
        {
            using var doc = WclParser.Parse("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }");
            Assert.False(doc.HasErrors());

            var blocks = doc.Blocks();
            Assert.Equal(3, blocks.Count);

            var servers = doc.BlocksOfType("server");
            Assert.Equal(2, servers.Count);
            Assert.Equal("server", servers[0].Kind);
        }

        [Fact]
        public void DiagnosticsOnValidInput()
        {
            using var doc = WclParser.Parse("x = 42");
            var diags = doc.Diagnostics;
            Assert.DoesNotContain(diags, d => d.IsError);
        }

        [Fact]
        public void DocumentDispose()
        {
            var doc = WclParser.Parse("x = 1");
            doc.Dispose();
            // Double dispose should not throw
            doc.Dispose();

            // Access after dispose should throw
            Assert.Throws<ObjectDisposedException>(() => doc.Values);
        }

        [Fact]
        public async Task ConcurrentReads()
        {
            using var doc = WclParser.Parse("x = 42\ny = \"hello\"");
            var tasks = new Task[10];
            for (int i = 0; i < 10; i++)
            {
                tasks[i] = Task.Run(() =>
                {
                    var values = doc.Values;
                    Assert.Equal(42L, values["x"].AsInt());
                });
            }
            await Task.WhenAll(tasks);
        }

        [Fact]
        public void FromString()
        {
            var result = WclParser.FromString<Dictionary<string, long>>("x = 10\ny = 20");
            Assert.Equal(10L, result["x"]);
            Assert.Equal(20L, result["y"]);
        }

        [Fact]
        public void BlocksWithDecorators()
        {
            using var doc = WclParser.Parse(@"
                @deprecated(""use new-svc"")
                server old-svc {
                    port = 80
                }
            ");
            Assert.False(doc.HasErrors());

            var blocks = doc.Blocks();
            Assert.Single(blocks);
            Assert.Equal("server", blocks[0].Kind);
            Assert.Equal("old-svc", blocks[0].Id);
            Assert.True(blocks[0].HasDecorator("deprecated"));
            Assert.NotNull(blocks[0].GetDecorator("deprecated"));
        }

        [Fact]
        public void NestedBlocks()
        {
            using var doc = WclParser.Parse(@"
                server main {
                    port = 8080
                    logging {
                        level = ""info""
                    }
                }
            ");
            Assert.False(doc.HasErrors());

            var blocks = doc.Blocks();
            Assert.Single(blocks);
            Assert.Equal("server", blocks[0].Kind);
            Assert.NotEmpty(blocks[0].Children);
            Assert.Equal("logging", blocks[0].Children[0].Kind);
        }

        [Fact]
        public void ListValues()
        {
            using var doc = WclParser.Parse("tags = [\"a\", \"b\", \"c\"]");
            Assert.False(doc.HasErrors());

            var tags = doc.Values["tags"];
            Assert.Equal(WclValueKind.List, tags.Kind);
            Assert.Equal(3, tags.AsList().Count);
            Assert.Equal("a", tags.AsList()[0].AsString());
        }

        [Fact]
        public void BlockAttributes()
        {
            using var doc = WclParser.Parse(@"
                server web {
                    port = 8080
                    host = ""localhost""
                    debug = false
                }
            ");
            Assert.False(doc.HasErrors());

            var servers = doc.BlocksOfType("server");
            Assert.Single(servers);
            var s = servers[0];
            Assert.Equal("web", s.Id);
            Assert.Equal(WclValue.NewInt(8080), s.Get("port"));
            Assert.Equal(WclValue.NewString("localhost"), s.Get("host"));
            Assert.Equal(WclValue.NewBool(false), s.Get("debug"));
            Assert.Null(s.Get("nonexistent"));
        }

        [Fact]
        public void MapValues()
        {
            using var doc = WclParser.Parse("config = { a = 1, b = 2 }");
            Assert.False(doc.HasErrors());

            var config = doc.Values["config"];
            Assert.Equal(WclValueKind.Map, config.Kind);
            var map = config.AsMap();
            Assert.Equal(2, map.Count);
            Assert.Equal(1L, map["a"].AsInt());
            Assert.Equal(2L, map["b"].AsInt());
        }

        [Fact]
        public void NullValues()
        {
            using var doc = WclParser.Parse("x = null");
            Assert.False(doc.HasErrors());
            Assert.True(doc.Values["x"].IsNull);
        }

        [Fact]
        public void BoolAndFloatValues()
        {
            using var doc = WclParser.Parse("flag = true\npi = 3.14");
            Assert.False(doc.HasErrors());
            Assert.True(doc.Values["flag"].AsBool());
            Assert.Equal(3.14, doc.Values["pi"].AsFloat());
        }

        [Fact]
        public void QueryById()
        {
            using var doc = WclParser.Parse(@"
                server api { port = 8080 }
                server web { port = 9090 }
            ");
            Assert.False(doc.HasErrors());

            var result = doc.Query("server#api");
            // Query returns a list of matching blocks
            var list = result.AsList();
            Assert.Single(list);
            var br = list[0].AsBlockRef();
            Assert.Equal("api", br.Id);
        }
    }
}
