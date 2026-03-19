using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;
using Xunit;

namespace Wcl.Tests.Eval
{
    public class ScopeTests
    {
        [Fact]
        public void ResolveInCurrentScope()
        {
            var arena = new ScopeArena();
            var scope = arena.CreateScope(ScopeKind.Module, null);
            arena.AddEntry(scope, new ScopeEntry("x", ScopeEntryKind.LetBinding, WclValue.NewInt(42), Span.Dummy()));
            Assert.Equal(WclValue.NewInt(42), arena.Resolve(scope, "x"));
        }

        [Fact]
        public void ResolveInParentScope()
        {
            var arena = new ScopeArena();
            var parent = arena.CreateScope(ScopeKind.Module, null);
            arena.AddEntry(parent, new ScopeEntry("x", ScopeEntryKind.LetBinding, WclValue.NewInt(42), Span.Dummy()));
            var child = arena.CreateScope(ScopeKind.Block, parent);
            Assert.Equal(WclValue.NewInt(42), arena.Resolve(child, "x"));
        }

        [Fact]
        public void ShadowingInChildScope()
        {
            var arena = new ScopeArena();
            var parent = arena.CreateScope(ScopeKind.Module, null);
            arena.AddEntry(parent, new ScopeEntry("x", ScopeEntryKind.LetBinding, WclValue.NewInt(1), Span.Dummy()));
            var child = arena.CreateScope(ScopeKind.Block, parent);
            arena.AddEntry(child, new ScopeEntry("x", ScopeEntryKind.LetBinding, WclValue.NewInt(2), Span.Dummy()));
            Assert.Equal(WclValue.NewInt(2), arena.Resolve(child, "x"));
            Assert.Equal(WclValue.NewInt(1), arena.Resolve(parent, "x"));
        }

        [Fact]
        public void ResolveUndefinedReturnsNull()
        {
            var arena = new ScopeArena();
            var scope = arena.CreateScope(ScopeKind.Module, null);
            Assert.Null(arena.Resolve(scope, "undefined_var"));
        }
    }
}
