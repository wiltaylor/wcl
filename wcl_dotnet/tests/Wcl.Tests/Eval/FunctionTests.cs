using System.Collections.Generic;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class FunctionTests
    {
        [Fact]
        public void Upper() => Assert.Equal(WclValue.NewString("HELLO"), TestHelpers.Eval("upper(\"hello\")"));

        [Fact]
        public void Lower() => Assert.Equal(WclValue.NewString("hello"), TestHelpers.Eval("lower(\"HELLO\")"));

        [Fact]
        public void Trim() => Assert.Equal(WclValue.NewString("hi"), TestHelpers.Eval("trim(\"  hi  \")"));

        [Fact]
        public void Replace() => Assert.Equal(WclValue.NewString("hXllo"), TestHelpers.Eval("replace(\"hello\", \"e\", \"X\")"));

        [Fact]
        public void Split()
        {
            var result = TestHelpers.Eval("split(\"a,b,c\", \",\")");
            Assert.Equal(3, result.AsList().Count);
            Assert.Equal("a", result.AsList()[0].AsString());
        }

        [Fact]
        public void Join() => Assert.Equal(WclValue.NewString("1-2-3"),
            TestHelpers.Eval("join([1, 2, 3], \"-\")"));

        [Fact]
        public void StartsWith() => Assert.Equal(WclValue.NewBool(true),
            TestHelpers.Eval("starts_with(\"hello\", \"he\")"));

        [Fact]
        public void EndsWith() => Assert.Equal(WclValue.NewBool(true),
            TestHelpers.Eval("ends_with(\"hello\", \"lo\")"));

        [Fact]
        public void Contains() => Assert.Equal(WclValue.NewBool(true),
            TestHelpers.Eval("contains(\"hello\", \"ell\")"));

        [Fact]
        public void Len()
        {
            Assert.Equal(WclValue.NewInt(3), TestHelpers.Eval("len([1, 2, 3])"));
            Assert.Equal(WclValue.NewInt(5), TestHelpers.Eval("len(\"hello\")"));
        }

        [Fact]
        public void Keys()
        {
            var result = TestHelpers.Eval("keys({ a = 1, b = 2 })");
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void Values()
        {
            var result = TestHelpers.Eval("values({ a = 1, b = 2 })");
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void Flatten()
        {
            var result = TestHelpers.Eval("flatten([[1, 2], [3, 4]])");
            Assert.Equal(4, result.AsList().Count);
        }

        [Fact]
        public void Sort()
        {
            var result = TestHelpers.Eval("sort([3, 1, 2])");
            Assert.Equal(WclValue.NewInt(1), result.AsList()[0]);
            Assert.Equal(WclValue.NewInt(2), result.AsList()[1]);
            Assert.Equal(WclValue.NewInt(3), result.AsList()[2]);
        }

        [Fact]
        public void Reverse()
        {
            var result = TestHelpers.Eval("reverse([1, 2, 3])");
            Assert.Equal(WclValue.NewInt(3), result.AsList()[0]);
        }

        [Fact]
        public void Range()
        {
            var result = TestHelpers.Eval("range(0, 3)");
            Assert.Equal(3, result.AsList().Count);
        }

        [Fact]
        public void Sum() => Assert.Equal(WclValue.NewInt(6), TestHelpers.Eval("sum([1, 2, 3])"));

        [Fact]
        public void Avg() => Assert.Equal(WclValue.NewFloat(2.0), TestHelpers.Eval("avg([1, 2, 3])"));

        [Fact]
        public void Abs() => Assert.Equal(WclValue.NewInt(5), TestHelpers.Eval("abs(-5)"));

        [Fact]
        public void MinMax()
        {
            Assert.Equal(WclValue.NewInt(1), TestHelpers.Eval("min(1, 2)"));
            Assert.Equal(WclValue.NewInt(2), TestHelpers.Eval("max(1, 2)"));
        }

        [Fact]
        public void FloorCeilRound()
        {
            Assert.Equal(WclValue.NewInt(3), TestHelpers.Eval("floor(3.7)"));
            Assert.Equal(WclValue.NewInt(4), TestHelpers.Eval("ceil(3.2)"));
            Assert.Equal(WclValue.NewInt(4), TestHelpers.Eval("round(3.5)"));
        }

        [Fact]
        public void TypeOf() => Assert.Equal(WclValue.NewString("int"), TestHelpers.Eval("type_of(42)"));

        [Fact]
        public void ToString_() => Assert.Equal(WclValue.NewString("42"), TestHelpers.Eval("to_string(42)"));

        [Fact]
        public void ToInt() => Assert.Equal(WclValue.NewInt(42), TestHelpers.Eval("to_int(\"42\")"));

        [Fact]
        public void ToFloat() => Assert.Equal(WclValue.NewFloat(42.0), TestHelpers.Eval("to_float(42)"));

        [Fact]
        public void Sha256()
        {
            var result = TestHelpers.Eval("sha256(\"hello\")");
            Assert.Equal(WclValueKind.String, result.Kind);
            Assert.Equal(64, result.AsString().Length);
        }

        [Fact]
        public void Base64()
        {
            Assert.Equal(WclValue.NewString("aGVsbG8="), TestHelpers.Eval("base64_encode(\"hello\")"));
            Assert.Equal(WclValue.NewString("hello"), TestHelpers.Eval("base64_decode(\"aGVsbG8=\")"));
        }

        [Fact]
        public void RegexMatch()
        {
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("regex_match(\"hello123\", \"[a-z]+\\\\d+\")"));
        }

        [Fact]
        public void MapHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = map([1, 2, 3], x => x * 2)");
            var result = doc.Values["result"];
            Assert.Equal(3, result.AsList().Count);
            Assert.Equal(WclValue.NewInt(2), result.AsList()[0]);
            Assert.Equal(WclValue.NewInt(4), result.AsList()[1]);
            Assert.Equal(WclValue.NewInt(6), result.AsList()[2]);
        }

        [Fact]
        public void FilterHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = filter([1, 2, 3, 4, 5], x => x > 3)");
            var result = doc.Values["result"];
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void EveryHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = every([2, 4, 6], x => x > 0)");
            Assert.Equal(WclValue.NewBool(true), doc.Values["result"]);
        }

        [Fact]
        public void SomeHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = some([1, 2, 3], x => x > 2)");
            Assert.Equal(WclValue.NewBool(true), doc.Values["result"]);
        }

        [Fact]
        public void ReduceHigherOrder()
        {
            var doc = TestHelpers.ParseDoc("result = reduce([1, 2, 3], 0, (acc, x) => acc + x)");
            Assert.Equal(WclValue.NewInt(6), doc.Values["result"]);
        }

        [Fact]
        public void Distinct()
        {
            var result = TestHelpers.Eval("distinct([1, 2, 1, 3, 2])");
            Assert.Equal(3, result.AsList().Count);
        }

        [Fact]
        public void IndexOf()
        {
            Assert.Equal(WclValue.NewInt(1), TestHelpers.Eval("index_of([10, 20, 30], 20)"));
            Assert.Equal(WclValue.NewInt(-1), TestHelpers.Eval("index_of([10, 20, 30], 99)"));
        }

        [Fact]
        public void JsonEncode()
        {
            var result = TestHelpers.Eval("json_encode({ name = \"test\", count = 42 })");
            Assert.Contains("\"name\"", result.AsString());
            Assert.Contains("42", result.AsString());
        }
    }
}
