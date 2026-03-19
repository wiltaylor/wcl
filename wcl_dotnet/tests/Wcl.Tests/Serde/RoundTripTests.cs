using System.Collections.Generic;
using Wcl.Core;
using Wcl.Eval;
using Wcl.Serde;
using Xunit;

namespace Wcl.Tests.Serde
{
    public class RoundTripTests
    {
        [Fact]
        public void IntRoundTrip()
        {
            var original = 42;
            var serialized = WclSerializer.Serialize(original);
            Assert.Equal("42", serialized);
        }

        [Fact]
        public void StringRoundTrip()
        {
            var serialized = WclSerializer.Serialize("hello world");
            Assert.Equal("\"hello world\"", serialized);
        }

        [Fact]
        public void BoolRoundTrip()
        {
            Assert.Equal("true", WclSerializer.Serialize(true));
            Assert.Equal("false", WclSerializer.Serialize(false));
        }

        [Fact]
        public void ListRoundTrip()
        {
            var list = new List<int> { 1, 2, 3 };
            var serialized = WclSerializer.Serialize(list);
            Assert.Equal("[1, 2, 3]", serialized);
        }

        [Fact]
        public void DictRoundTrip()
        {
            var dict = new Dictionary<string, int> { ["a"] = 1, ["b"] = 2 };
            var serialized = WclSerializer.Serialize(dict);
            Assert.Contains("a", serialized);
            Assert.Contains("b", serialized);
        }

        [Fact]
        public void DeserializeFromWclSource()
        {
            var result = WclParser.FromString<Dictionary<string, long>>("x = 10\ny = 20");
            Assert.Equal(10L, result["x"]);
            Assert.Equal(20L, result["y"]);
        }

        [Fact]
        public void DeserializeStringValues()
        {
            var result = WclParser.FromString<Dictionary<string, string>>("name = \"test\"\nhost = \"localhost\"");
            Assert.Equal("test", result["name"]);
            Assert.Equal("localhost", result["host"]);
        }
    }
}
