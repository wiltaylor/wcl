using System.Collections.Generic;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval
{
    public enum ScopeEntryKind
    {
        LetBinding,
        Attribute,
        BlockChild,
        Import,
        ForIterator,
        ForIndex,
        Parameter,
    }

    public class ScopeEntry
    {
        public string Name { get; set; }
        public ScopeEntryKind Kind { get; set; }
        public WclValue? Value { get; set; }
        public Span Span { get; set; }
        public HashSet<string> Dependencies { get; set; }
        public bool Evaluated { get; set; }
        public int ReadCount { get; set; }

        public ScopeEntry(string name, ScopeEntryKind kind, WclValue? value, Span span)
        {
            Name = name; Kind = kind; Value = value; Span = span;
            Dependencies = new HashSet<string>();
            Evaluated = value != null;
        }
    }

    public class Scope
    {
        public ScopeId Id { get; }
        public ScopeKind Kind { get; }
        public ScopeId? Parent { get; }
        public List<ScopeEntry> Entries { get; } = new List<ScopeEntry>();

        public Scope(ScopeId id, ScopeKind kind, ScopeId? parent)
        {
            Id = id; Kind = kind; Parent = parent;
        }
    }

    public class ScopeArena
    {
        private readonly List<Scope> _scopes = new List<Scope>();

        public ScopeId CreateScope(ScopeKind kind, ScopeId? parent)
        {
            var id = new ScopeId((uint)_scopes.Count);
            _scopes.Add(new Scope(id, kind, parent));
            return id;
        }

        public Scope Get(ScopeId id) => _scopes[(int)id.Value];

        public void AddEntry(ScopeId scopeId, ScopeEntry entry)
        {
            _scopes[(int)scopeId.Value].Entries.Add(entry);
        }

        public WclValue? Resolve(ScopeId scopeId, string name)
        {
            var current = (ScopeId?)scopeId;
            while (current.HasValue)
            {
                var scope = _scopes[(int)current.Value.Value];
                for (int i = scope.Entries.Count - 1; i >= 0; i--)
                {
                    if (scope.Entries[i].Name == name)
                    {
                        scope.Entries[i].ReadCount++;
                        return scope.Entries[i].Value;
                    }
                }
                current = scope.Parent;
            }
            return null;
        }

        public bool HasEntry(ScopeId scopeId, string name)
        {
            var current = (ScopeId?)scopeId;
            while (current.HasValue)
            {
                var scope = _scopes[(int)current.Value.Value];
                foreach (var entry in scope.Entries)
                    if (entry.Name == name) return true;
                current = scope.Parent;
            }
            return false;
        }

        public int Count => _scopes.Count;
    }
}
