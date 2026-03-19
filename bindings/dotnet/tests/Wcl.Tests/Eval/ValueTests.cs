using System.Collections.Generic;
using Wcl.Core;
using Wcl.Eval;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class ValueTests
    {
        [Fact]
        public void TypeNames()
        {
            Assert.Equal("string", WclValue.NewString("hi").TypeName);
            Assert.Equal("int", WclValue.NewInt(1).TypeName);
            Assert.Equal("float", WclValue.NewFloat(1.0).TypeName);
            Assert.Equal("bool", WclValue.NewBool(true).TypeName);
            Assert.Equal("null", WclValue.Null.TypeName);
            Assert.Equal("identifier", WclValue.NewIdentifier("svc").TypeName);
            Assert.Equal("list", WclValue.NewList(new List<WclValue>()).TypeName);
            Assert.Equal("map", WclValue.NewMap(new OrderedMap<string, WclValue>()).TypeName);
            Assert.Equal("set", WclValue.NewSet(new List<WclValue>()).TypeName);
        }

        [Fact]
        public void EqualitySameVariant()
        {
            Assert.Equal(WclValue.NewInt(42), WclValue.NewInt(42));
            Assert.Equal(WclValue.NewString("hello"), WclValue.NewString("hello"));
            Assert.Equal(WclValue.NewBool(false), WclValue.NewBool(false));
            Assert.Equal(WclValue.Null, WclValue.Null);
            Assert.Equal(WclValue.NewIdentifier("svc"), WclValue.NewIdentifier("svc"));
        }

        [Fact]
        public void EqualityDifferentValue()
        {
            Assert.NotEqual(WclValue.NewInt(1), WclValue.NewInt(2));
            Assert.NotEqual(WclValue.NewString("a"), WclValue.NewString("b"));
        }

        [Fact]
        public void EqualityCrossVariantFalse()
        {
            // Int and Float are NOT equal even with same magnitude
            Assert.NotEqual(WclValue.NewInt(1), WclValue.NewFloat(1.0));
            // String and Identifier NOT equal
            Assert.NotEqual(WclValue.NewString("foo"), WclValue.NewIdentifier("foo"));
        }

        [Fact]
        public void EqualityListAndMap()
        {
            var a = WclValue.NewList(new List<WclValue> { WclValue.NewInt(1), WclValue.NewInt(2) });
            var b = WclValue.NewList(new List<WclValue> { WclValue.NewInt(1), WclValue.NewInt(2) });
            Assert.Equal(a, b);

            var m1 = new OrderedMap<string, WclValue>();
            m1["k"] = WclValue.NewBool(true);
            var m2 = new OrderedMap<string, WclValue>();
            m2["k"] = WclValue.NewBool(true);
            Assert.Equal(WclValue.NewMap(m1), WclValue.NewMap(m2));
        }

        [Fact]
        public void InterpStringScalars()
        {
            Assert.Equal("hello", WclValue.NewString("hello").ToInterpString());
            Assert.Equal("42", WclValue.NewInt(42).ToInterpString());
            Assert.Equal("true", WclValue.NewBool(true).ToInterpString());
            Assert.Equal("null", WclValue.Null.ToInterpString());
        }

        [Fact]
        public void InterpStringNonScalarThrows()
        {
            Assert.Throws<System.InvalidOperationException>(() =>
                WclValue.NewList(new List<WclValue>()).ToInterpString());
        }

        [Fact]
        public void DisplayList()
        {
            var v = WclValue.NewList(new List<WclValue>
                { WclValue.NewInt(1), WclValue.NewInt(2), WclValue.NewInt(3) });
            Assert.Equal("[1, 2, 3]", v.ToString());
        }

        [Fact]
        public void DisplayNull()
        {
            Assert.Equal("null", WclValue.Null.ToString());
        }

        [Fact]
        public void DisplaySet()
        {
            var v = WclValue.NewSet(new List<WclValue>
                { WclValue.NewString("a"), WclValue.NewString("b") });
            Assert.Equal("set(a, b)", v.ToString());
        }
    }
}
