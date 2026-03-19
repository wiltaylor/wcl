using Wcl.Core;
using Xunit;

namespace Wcl.Tests.Core
{
    public class DiagnosticTests
    {
        [Fact]
        public void ErrorDiagnostic()
        {
            var d = Diagnostic.Error("test error", Span.Dummy());
            Assert.True(d.IsError);
            Assert.Equal(Severity.Error, d.Severity);
        }

        [Fact]
        public void WarningDiagnostic()
        {
            var d = Diagnostic.Warning("test warning", Span.Dummy());
            Assert.False(d.IsError);
            Assert.Equal(Severity.Warning, d.Severity);
        }

        [Fact]
        public void WithCode()
        {
            var d = Diagnostic.Error("test", Span.Dummy()).WithCode("E001");
            Assert.Equal("E001", d.Code);
        }

        [Fact]
        public void WithLabel()
        {
            var d = Diagnostic.Error("test", Span.Dummy()).WithLabel(Span.Dummy(), "here");
            Assert.Single(d.Labels);
            Assert.Equal("here", d.Labels[0].Message);
        }

        [Fact]
        public void DiagnosticBagHasErrors()
        {
            var bag = new DiagnosticBag();
            Assert.False(bag.HasErrors);
            bag.Error("err", Span.Dummy());
            Assert.True(bag.HasErrors);
        }

        [Fact]
        public void DiagnosticBagWarningNoError()
        {
            var bag = new DiagnosticBag();
            bag.Warning("warn", Span.Dummy());
            Assert.False(bag.HasErrors);
        }

        [Fact]
        public void DiagnosticBagMerge()
        {
            var a = new DiagnosticBag();
            a.Error("a", Span.Dummy());
            var b = new DiagnosticBag();
            b.Error("b", Span.Dummy());
            a.Merge(b);
            Assert.Equal(2, a.Count);
        }

        [Fact]
        public void DiagnosticToString()
        {
            var d = Diagnostic.Error("test", Span.Dummy()).WithCode("E042");
            Assert.Contains("E042", d.ToString());
        }

        [Fact]
        public void TriviaEmpty()
        {
            var t = Trivia.Empty();
            Assert.Empty(t.Comments);
            Assert.Equal(0, t.LeadingNewlines);
        }

        [Fact]
        public void FileIdEquality()
        {
            Assert.Equal(new FileId(1), new FileId(1));
            Assert.NotEqual(new FileId(1), new FileId(2));
        }

        [Fact]
        public void SpanLength()
        {
            var s = new Span(new FileId(0), 5, 15);
            Assert.Equal(10, s.Length);
        }
    }
}
