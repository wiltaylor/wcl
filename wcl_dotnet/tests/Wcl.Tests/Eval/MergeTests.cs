using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class MergeTests
    {
        [Fact]
        public void PartialBlocksMerged()
        {
            var doc = TestHelpers.ParseDoc(@"
                partial server main { port = 80 }
                partial server main { host = ""localhost"" }
            ");
            var servers = doc.BlocksOfType("server");
            Assert.Single(servers);
            Assert.Equal(2, servers[0].Body.Count); // port and host
        }

        [Fact]
        public void SinglePartialLosesFlag()
        {
            var doc = TestHelpers.ParseDoc("partial server main { port = 80 }");
            var servers = doc.BlocksOfType("server");
            Assert.Single(servers);
            Assert.False(servers[0].Partial);
        }

        [Fact]
        public void NonPartialBlocksUnchanged()
        {
            var doc = TestHelpers.ParseDoc("server a { port = 80 }\nserver b { port = 443 }");
            var servers = doc.BlocksOfType("server");
            Assert.Equal(2, servers.Count);
        }
    }
}
