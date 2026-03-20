using System.Collections.Generic;
using Wcl.Core;
using Wcl.Eval;
using Wcl.Serde;
using Xunit;

namespace Wcl.Tests.Serde
{
    public class SerdeTests
    {
        [Fact]
        public void DeserializePrimitives()
        {
            Assert.Equal("hello", WclDeserializer.FromValue<string>(WclValue.NewString("hello")));
            Assert.Equal(42L, WclDeserializer.FromValue<long>(WclValue.NewInt(42)));
            Assert.Equal(3.14, WclDeserializer.FromValue<double>(WclValue.NewFloat(3.14)));
            Assert.True(WclDeserializer.FromValue<bool>(WclValue.NewBool(true)));
        }

        [Fact]
        public void DeserializeList()
        {
            var val = WclValue.NewList(new List<WclValue>
                { WclValue.NewInt(1), WclValue.NewInt(2), WclValue.NewInt(3) });
            var result = WclDeserializer.FromValue<List<long>>(val);
            Assert.Equal(3, result.Count);
            Assert.Equal(1L, result[0]);
        }

        [Fact]
        public void DeserializeDict()
        {
            var map = new OrderedMap<string, WclValue>();
            map["a"] = WclValue.NewInt(1);
            map["b"] = WclValue.NewInt(2);
            var result = WclDeserializer.FromValue<Dictionary<string, long>>(WclValue.NewMap(map));
            Assert.Equal(1L, result["a"]);
            Assert.Equal(2L, result["b"]);
        }

        [Fact]
        public void DeserializeIntToFloat()
        {
            Assert.Equal(42.0, WclDeserializer.FromValue<double>(WclValue.NewInt(42)));
        }

        [Fact]
        public void DeserializeNullToNullable()
        {
            Assert.Null(WclDeserializer.FromValue<string?>(WclValue.Null));
        }

        [Fact]
        public void SerializeCompact()
        {
            var result = WclSerializer.Serialize(new Dictionary<string, object>
            {
                { "name", "test" },
                { "count", 42 }
            });
            Assert.Contains("name", result);
        }

        [Fact]
        public void SerializeString()
        {
            var result = WclSerializer.Serialize("hello");
            Assert.Equal("\"hello\"", result);
        }

        [Fact]
        public void SerializeBool()
        {
            Assert.Equal("true", WclSerializer.Serialize(true));
            Assert.Equal("false", WclSerializer.Serialize(false));
        }

        [Fact]
        public void SerializeNull()
        {
            Assert.Equal("null", WclSerializer.Serialize(null!));
        }
    }
}
