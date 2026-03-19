using System.Linq;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Schema
{
    public class SchemaTests
    {
        [Fact]
        public void MissingRequiredFieldE070()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" { port: int }
                config { }
            ");
            var e070 = doc.Diagnostics.Where(d => d.Code == "E070").ToList();
            Assert.NotEmpty(e070);
        }

        [Fact]
        public void TypeMismatchE071()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" { port: int }
                config { port = ""not_a_number"" }
            ");
            var e071 = doc.Diagnostics.Where(d => d.Code == "E071").ToList();
            Assert.NotEmpty(e071);
        }

        [Fact]
        public void UnknownAttrClosedSchemaE072()
        {
            var doc = TestHelpers.ParseDoc(@"
                @closed
                schema ""config"" { port: int }
                config { port = 8080
                    extra = true }
            ");
            var e072 = doc.Diagnostics.Where(d => d.Code == "E072").ToList();
            Assert.NotEmpty(e072);
        }

        [Fact]
        public void ValidSchemaNoErrors()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" { port: int }
                config { port = 8080 }
            ");
            var schemaErrors = doc.Diagnostics.Where(d =>
                d.Code == "E070" || d.Code == "E071" || d.Code == "E072").ToList();
            Assert.Empty(schemaErrors);
        }

        [Fact]
        public void OptionalFieldNotRequired()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" {
                    port: int
                    host: string @optional
                }
                config { port = 8080 }
            ");
            var e070 = doc.Diagnostics.Where(d => d.Code == "E070").ToList();
            Assert.Empty(e070);
        }

        [Fact]
        public void DuplicateSchemaE001()
        {
            var doc = TestHelpers.ParseDoc(@"
                schema ""config"" { port: int }
                schema ""config"" { host: string }
            ");
            var e001 = doc.Diagnostics.Where(d => d.Code == "E001").ToList();
            Assert.NotEmpty(e001);
        }

        [Fact]
        public void DuplicateBlockIdE030()
        {
            var doc = TestHelpers.ParseDoc(@"
                server main { port = 80 }
                server main { port = 443 }
            ");
            var e030 = doc.Diagnostics.Where(d => d.Code == "E030").ToList();
            Assert.NotEmpty(e030);
        }

        [Fact]
        public void TableColumnTypeE071()
        {
            var doc = TestHelpers.ParseDoc(@"
                table users {
                    name: string
                    port: int
                    | ""web"" | 8080 |
                    | ""api"" | ""bad"" |
                }
            ");
            var e071 = doc.Diagnostics.Where(d => d.Code == "E071").ToList();
            Assert.NotEmpty(e071);
        }

        [Fact]
        public void ValidTableNoErrors()
        {
            var doc = TestHelpers.ParseDoc(@"
                table users {
                    name: string
                    age: int
                    | ""Alice"" | 30 |
                    | ""Bob""   | 25 |
                }
            ");
            var e071 = doc.Diagnostics.Where(d => d.Code == "E071").ToList();
            Assert.Empty(e071);
        }

        [Fact]
        public void UnknownDecoratorE060()
        {
            var doc = TestHelpers.ParseDoc("@nonexistent\nserver main { port = 8080 }");
            var e060 = doc.Diagnostics.Where(d => d.Code == "E060").ToList();
            Assert.Single(e060);
        }

        [Fact]
        public void KnownDecoratorNoE060()
        {
            var doc = TestHelpers.ParseDoc("@deprecated(\"use new\")\nserver main { port = 8080 }");
            var e060 = doc.Diagnostics.Where(d => d.Code == "E060").ToList();
            Assert.Empty(e060);
        }

        [Fact]
        public void ValidationBlockPassing()
        {
            var doc = TestHelpers.ParseDoc(@"
                validation ""passes"" {
                    let x = 10
                    check = x > 0
                    message = ""x is not positive""
                }
            ");
            var valErrors = doc.Diagnostics.Where(d => d.Code == "E080").ToList();
            Assert.Empty(valErrors);
        }

        [Fact]
        public void ValidationBlockFailing()
        {
            var doc = TestHelpers.ParseDoc(@"
                validation ""fails"" {
                    let x = -5
                    check = x > 0
                    message = ""x is not positive""
                }
            ");
            var valErrors = doc.Diagnostics.Where(d => d.Code == "E080").ToList();
            Assert.NotEmpty(valErrors);
        }

        [Fact]
        public void ValidationBlockWarning()
        {
            var doc = TestHelpers.ParseDoc(@"
                @warning
                validation ""warns"" {
                    let x = -5
                    check = x > 0
                    message = ""x is not positive""
                }
            ");
            var valDiags = doc.Diagnostics.Where(d => d.Code == "E080").ToList();
            Assert.NotEmpty(valDiags);
            Assert.False(valDiags[0].IsError);
        }
    }
}
