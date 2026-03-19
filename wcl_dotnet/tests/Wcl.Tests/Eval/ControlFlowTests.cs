using System.Linq;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class ControlFlowTests
    {
        [Fact]
        public void ForLoopBasic()
        {
            var doc = TestHelpers.ParseDoc(@"
                for item in [1, 2, 3] {
                    entry { value = item }
                }
            ");
            Assert.Equal(3, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void ForLoopWithIndex()
        {
            var doc = TestHelpers.ParseDoc(@"
                for item, i in [""a"", ""b""] {
                    entry { value = item
                        idx = i }
                }
            ");
            Assert.Equal(2, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void NestedForLoops()
        {
            var doc = TestHelpers.ParseDoc(@"
                for x in [1, 2] {
                    for y in [""a"", ""b""] {
                        entry { xval = x }
                    }
                }
            ");
            Assert.Equal(4, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void ConditionalTrue()
        {
            var doc = TestHelpers.ParseDoc("if true { x = 42 }");
            Assert.True(doc.Values.ContainsKey("x"));
            Assert.Equal(WclValue.NewInt(42), doc.Values["x"]);
        }

        [Fact]
        public void ConditionalFalse()
        {
            var doc = TestHelpers.ParseDoc("if false { x = 42 }");
            Assert.False(doc.Values.ContainsKey("x"));
        }

        [Fact]
        public void ConditionalElseIf()
        {
            var doc = TestHelpers.ParseDoc(@"
                if false { x = 1 }
                else if true { x = 2 }
                else { x = 3 }
            ");
            Assert.Equal(WclValue.NewInt(2), doc.Values["x"]);
        }

        [Fact]
        public void ForLoopOverLetBinding()
        {
            var doc = TestHelpers.ParseDoc(@"
                let items = [10, 20, 30]
                for item in items {
                    entry { value = item }
                }
            ");
            Assert.Equal(3, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void ForLoopWithExpression()
        {
            var doc = TestHelpers.ParseDoc(@"
                for item in range(0, 5) {
                    entry { value = item }
                }
            ");
            Assert.Equal(5, doc.BlocksOfType("entry").Count);
        }

        [Fact]
        public void ConditionalInBlock()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main {
                    if true {
                        port = 8080
                    }
                }
            ");
            Assert.False(doc.HasErrors());
        }
    }
}
