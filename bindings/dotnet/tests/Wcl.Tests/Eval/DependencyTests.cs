using System.Linq;
using Wcl.Eval;
using Wcl.Tests.Helpers;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class DependencyTests
    {
        [Fact]
        public void ForwardReferenceResolved()
        {
            // b depends on a, but a is declared first — should work in any order
            var doc = TestHelpers.ParseDoc("let a = 10\nresult = a + 1");
            Assert.Equal(WclValue.NewInt(11), doc.Values["result"]);
        }

        [Fact]
        public void OutOfOrderLetBindings()
        {
            // result depends on b, b depends on a — topo sort should handle this
            var doc = TestHelpers.ParseDoc("result = b * 2\nlet b = a + 1\nlet a = 5");
            Assert.Equal(WclValue.NewInt(12), doc.Values["result"]);
        }

        [Fact]
        public void CyclicDependencyDetected()
        {
            var doc = TestHelpers.ParseDoc("let a = b\nlet b = a");
            var e041 = doc.Diagnostics.Where(d => d.Code == "E041").ToList();
            Assert.NotEmpty(e041);
        }

        [Fact]
        public void FindDependenciesBasic()
        {
            var (ast, _) = Wcl.Core.Parser.WclParser.Parse("result = a + b * c", new Wcl.Core.FileId(0));
            var attr = ((Wcl.Core.Ast.BodyDocItem)ast.Items[0]).BodyItem as Wcl.Core.Ast.AttributeItem;
            var deps = Evaluator.FindDependencies(attr!.Attribute.Value);
            Assert.Contains("a", deps);
            Assert.Contains("b", deps);
            Assert.Contains("c", deps);
        }

        [Fact]
        public void FindDependenciesNested()
        {
            var (ast, _) = Wcl.Core.Parser.WclParser.Parse("result = foo(x.y, [z])", new Wcl.Core.FileId(0));
            var attr = ((Wcl.Core.Ast.BodyDocItem)ast.Items[0]).BodyItem as Wcl.Core.Ast.AttributeItem;
            var deps = Evaluator.FindDependencies(attr!.Attribute.Value);
            Assert.Contains("foo", deps);
            Assert.Contains("x", deps);
            Assert.Contains("z", deps);
        }

        [Fact]
        public void ShadowingDetectedAcrossScopes()
        {
            // Shadowing check is in ScopeArena.CheckShadowing
            var arena = new ScopeArena();
            var parent = arena.CreateScope(Wcl.Core.Ast.ScopeKind.Module, null);
            arena.AddEntry(parent, new ScopeEntry("x", ScopeEntryKind.LetBinding,
                WclValue.NewInt(1), Wcl.Core.Span.Dummy()));
            var child = arena.CreateScope(Wcl.Core.Ast.ScopeKind.Block, parent);

            var shadowedSpan = arena.CheckShadowing(child, "x");
            Assert.NotNull(shadowedSpan);
        }

        [Fact]
        public void UnusedVariableWarning()
        {
            var doc = TestHelpers.ParseDoc("let unused = 42\nx = 1");
            var w002 = doc.Diagnostics.Where(d => d.Code == "W002").ToList();
            Assert.NotEmpty(w002);
            Assert.Contains("unused", w002[0].Message);
        }

        [Fact]
        public void UsedVariableNoWarning()
        {
            var doc = TestHelpers.ParseDoc("let used = 42\nx = used");
            var w002 = doc.Diagnostics.Where(d => d.Code == "W002").ToList();
            Assert.Empty(w002);
        }

        [Fact]
        public void TopoSortBasic()
        {
            var arena = new ScopeArena();
            var scope = arena.CreateScope(Wcl.Core.Ast.ScopeKind.Module, null);
            var a = new ScopeEntry("a", ScopeEntryKind.LetBinding, null, Wcl.Core.Span.Dummy());
            var b = new ScopeEntry("b", ScopeEntryKind.LetBinding, null, Wcl.Core.Span.Dummy());
            b.Dependencies.Add("a");
            arena.AddEntry(scope, a);
            arena.AddEntry(scope, b);

            var (order, cycle) = arena.TopoSort(scope);
            Assert.NotNull(order);
            Assert.Null(cycle);
            Assert.Equal("a", order![0]);
            Assert.Equal("b", order[1]);
        }

        [Fact]
        public void TopoSortDetectsCycle()
        {
            var arena = new ScopeArena();
            var scope = arena.CreateScope(Wcl.Core.Ast.ScopeKind.Module, null);
            var a = new ScopeEntry("a", ScopeEntryKind.LetBinding, null, Wcl.Core.Span.Dummy());
            a.Dependencies.Add("b");
            var b = new ScopeEntry("b", ScopeEntryKind.LetBinding, null, Wcl.Core.Span.Dummy());
            b.Dependencies.Add("a");
            arena.AddEntry(scope, a);
            arena.AddEntry(scope, b);

            var (order, cycle) = arena.TopoSort(scope);
            Assert.Null(order);
            Assert.NotNull(cycle);
            Assert.True(cycle!.Count >= 2);
        }
    }
}
