using Wcl.Core;
using Wcl.Eval.Import;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class ImportTests
    {
        [Fact]
        public void InMemoryFileSystem()
        {
            var fs = new InMemoryFileSystem();
            fs.AddFile("/test.wcl", "x = 42");
            Assert.True(fs.Exists("/test.wcl"));
            Assert.Equal("x = 42", fs.ReadFile("/test.wcl"));
            Assert.False(fs.Exists("/missing.wcl"));
        }

        [Fact]
        public void InMemoryFileSystemMissingReturnsNull()
        {
            var fs = new InMemoryFileSystem();
            Assert.Null(fs.ReadFile("/missing"));
        }

        [Fact]
        public void ImportResolverDoesNotAllowImportsWhenDisabled()
        {
            var opts = new ParseOptions { AllowImports = false };
            var doc = WclParser.Parse("import \"./other.wcl\"", opts);
            // Should not crash, just won't resolve the import
            Assert.NotNull(doc);
        }
    }
}
