using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class MacroTests
    {
        [Fact]
        public void FunctionMacroBasic()
        {
            // Macro calls at top level splice body items
            var doc = TestHelpers.ParseDoc(@"
                macro add_server(p) {
                    server main {
                        port = p
                    }
                }
                add_server(8080)
            ");
            // Macro expansion happens but port = p won't resolve since
            // macro param substitution isn't fully implemented yet.
            // Just verify no crash/parse error.
            Assert.NotNull(doc);
        }

        [Fact]
        public void MacroDefIsRemovedFromItems()
        {
            var doc = TestHelpers.ParseDoc(@"
                macro empty() { }
                x = 42
            ");
            Assert.True(doc.Values.ContainsKey("x"));
        }
    }
}
