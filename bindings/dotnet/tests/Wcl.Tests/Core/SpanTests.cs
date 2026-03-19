using Wcl.Core;
using Xunit;

namespace Wcl.Tests.Core
{
    public class SpanTests
    {
        [Fact]
        public void DummySpan()
        {
            var span = Span.Dummy();
            Assert.Equal(0, span.Start);
            Assert.Equal(0, span.End);
        }

        [Fact]
        public void MergeSpans()
        {
            var a = new Span(new FileId(0), 5, 10);
            var b = new Span(new FileId(0), 15, 20);
            var merged = a.Merge(b);
            Assert.Equal(5, merged.Start);
            Assert.Equal(20, merged.End);
        }

        [Fact]
        public void SourceFileLineCol()
        {
            var sf = new SourceFile(new FileId(0), "test", "line1\nline2\nline3");
            Assert.Equal((1, 1), sf.LineCol(0));
            Assert.Equal((2, 1), sf.LineCol(6));
            Assert.Equal((3, 1), sf.LineCol(12));
        }

        [Fact]
        public void SourceMapAddAndGet()
        {
            var sm = new SourceMap();
            var id = sm.AddFile("test.wcl", "content");
            var file = sm.GetFile(id);
            Assert.NotNull(file);
            Assert.Equal("test.wcl", file!.Path);
        }

        [Fact]
        public void OrderedMapInsertionOrder()
        {
            var map = new OrderedMap<string, int>();
            map["b"] = 2;
            map["a"] = 1;
            map["c"] = 3;
            Assert.Equal(3, map.Count);
            Assert.Equal("b", map.Keys[0]);
            Assert.Equal("a", map.Keys[1]);
            Assert.Equal("c", map.Keys[2]);
        }

        [Fact]
        public void OrderedMapOverwrite()
        {
            var map = new OrderedMap<string, int>();
            map["a"] = 1;
            map["a"] = 2;
            Assert.Equal(1, map.Count);
            Assert.Equal(2, map["a"]);
        }

        [Fact]
        public void OrderedMapRemove()
        {
            var map = new OrderedMap<string, int>();
            map["a"] = 1;
            map["b"] = 2;
            map["c"] = 3;
            map.Remove("b");
            Assert.Equal(2, map.Count);
            Assert.Equal("a", map.Keys[0]);
            Assert.Equal("c", map.Keys[1]);
        }
    }
}
