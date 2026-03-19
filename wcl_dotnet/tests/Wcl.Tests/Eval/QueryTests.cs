using System.Collections.Generic;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class QueryTests
    {
        [Fact]
        public void KindSelector()
        {
            var doc = TestHelpers.ParseDoc("service { port = 80 }\nservice { port = 443 }\ndatabase { port = 5432 }");
            var result = doc.Query("service");
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void Projection()
        {
            var doc = TestHelpers.ParseDoc("service { port = 80 }\nservice { port = 443 }");
            var result = doc.Query("service | .port");
            Assert.Equal(WclValue.NewList(new List<WclValue>
                { WclValue.NewInt(80), WclValue.NewInt(443) }), result);
        }

        [Fact]
        public void AttrComparison()
        {
            var doc = TestHelpers.ParseDoc("service { port = 80 }\nservice { port = 443 }");
            var result = doc.Query("service | .port > 100");
            Assert.Single(result.AsList());
        }

        [Fact]
        public void WildcardSelector()
        {
            var doc = TestHelpers.ParseDoc("a { x = 1 }\nb { x = 2 }");
            var result = doc.Query("*");
            Assert.Equal(2, result.AsList().Count);
        }

        [Fact]
        public void HasDecoratorFilter()
        {
            var doc = TestHelpers.ParseDoc("@deprecated(\"old\")\nservice { port = 80 }\nservice { port = 443 }");
            var result = doc.Query("service | has(@deprecated)");
            Assert.Single(result.AsList());
        }

        [Fact]
        public void HasAttrFilter()
        {
            var doc = TestHelpers.ParseDoc("service { port = 80 }\nservice { host = \"localhost\" }");
            var result = doc.Query("service | has(.port)");
            Assert.Single(result.AsList());
        }

        [Fact]
        public void EmptyResult()
        {
            var doc = TestHelpers.ParseDoc("service { port = 80 }");
            var result = doc.Query("database");
            Assert.Empty(result.AsList());
        }
    }
}
