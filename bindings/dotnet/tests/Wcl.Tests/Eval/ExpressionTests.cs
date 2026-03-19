using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class ExpressionTests
    {
        [Fact]
        public void IntArithmetic()
        {
            Assert.Equal(WclValue.NewInt(7), TestHelpers.Eval("3 + 4"));
            Assert.Equal(WclValue.NewInt(6), TestHelpers.Eval("2 * 3"));
            Assert.Equal(WclValue.NewInt(1), TestHelpers.Eval("5 - 4"));
            Assert.Equal(WclValue.NewInt(2), TestHelpers.Eval("10 / 5"));
            Assert.Equal(WclValue.NewInt(1), TestHelpers.Eval("7 % 3"));
        }

        [Fact]
        public void FloatPromotion()
        {
            var result = TestHelpers.Eval("1 + 2.0");
            Assert.Equal(WclValueKind.Float, result.Kind);
            Assert.Equal(3.0, result.AsFloat());
        }

        [Fact]
        public void StringConcat()
        {
            Assert.Equal(WclValue.NewString("hello world"),
                TestHelpers.Eval("\"hello\" + \" world\""));
        }

        [Fact]
        public void Comparison()
        {
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("1 < 2"));
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("2 < 1"));
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("1 == 1"));
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("1 != 2"));
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("3 >= 3"));
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("3 <= 3"));
        }

        [Fact]
        public void BooleanShortCircuit()
        {
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("true && false"));
            Assert.Equal(WclValue.NewBool(true), TestHelpers.Eval("true || false"));
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("false && true"));
        }

        [Fact]
        public void UnaryOps()
        {
            Assert.Equal(WclValue.NewBool(false), TestHelpers.Eval("!true"));
            Assert.Equal(WclValue.NewInt(-5), TestHelpers.Eval("-5"));
        }

        [Fact]
        public void TernaryExpr()
        {
            Assert.Equal(WclValue.NewInt(1), TestHelpers.Eval("true ? 1 : 2"));
            Assert.Equal(WclValue.NewInt(2), TestHelpers.Eval("false ? 1 : 2"));
        }

        [Fact]
        public void ListLiteral()
        {
            var result = TestHelpers.Eval("[1, 2, 3]");
            Assert.Equal(WclValueKind.List, result.Kind);
            Assert.Equal(3, result.AsList().Count);
        }

        [Fact]
        public void MapLiteral()
        {
            var result = TestHelpers.Eval("{ x = 1, y = 2 }");
            Assert.Equal(WclValueKind.Map, result.Kind);
            Assert.Equal(2, result.AsMap().Count);
        }

        [Fact]
        public void MemberAccess()
        {
            var doc = TestHelpers.ParseDoc("let m = { x = 42 }\nresult = m.x");
            Assert.Equal(WclValue.NewInt(42), doc.Values["result"]);
        }

        [Fact]
        public void IndexAccess()
        {
            var doc = TestHelpers.ParseDoc("let l = [10, 20, 30]\nresult = l[1]");
            Assert.Equal(WclValue.NewInt(20), doc.Values["result"]);
        }

        [Fact]
        public void BlockExpr()
        {
            Assert.Equal(WclValue.NewInt(3), TestHelpers.Eval("{ let x = 1\n let y = 2\n x + y }"));
        }

        [Fact]
        public void NullLiteral()
        {
            Assert.Equal(WclValue.Null, TestHelpers.Eval("null"));
        }

        [Fact]
        public void HeredocString()
        {
            var doc = TestHelpers.ParseDoc("text = <<EOF\nhello\nworld\nEOF");
            Assert.True(doc.Values.ContainsKey("text"));
            Assert.Contains("hello", doc.Values["text"].AsString());
        }

        [Fact]
        public void StringInterpolation()
        {
            var doc = TestHelpers.ParseDoc("let name = \"world\"\nresult = \"hello ${name}\"");
            Assert.Equal("hello world", doc.Values["result"].AsString());
        }

        [Fact]
        public void LetBindingUsedInExpression()
        {
            var doc = TestHelpers.ParseDoc("let x = 10\nresult = x * 2");
            Assert.Equal(WclValue.NewInt(20), doc.Values["result"]);
        }
    }
}
