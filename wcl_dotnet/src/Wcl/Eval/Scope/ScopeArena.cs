using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval
{
    public enum ScopeEntryKind
    {
        LetBinding,
        ExportLet,
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
        private readonly List<ScopeEntry> _entries = new List<ScopeEntry>();
        private readonly Dictionary<string, int> _index = new Dictionary<string, int>();
        public List<ScopeId> Children { get; } = new List<ScopeId>();

        public Scope(ScopeId id, ScopeKind kind, ScopeId? parent)
        {
            Id = id; Kind = kind; Parent = parent;
        }

        public IReadOnlyList<ScopeEntry> Entries => _entries;

        public void AddEntry(ScopeEntry entry)
        {
            if (_index.TryGetValue(entry.Name, out int idx))
            {
                _entries[idx] = entry;
            }
            else
            {
                _index[entry.Name] = _entries.Count;
                _entries.Add(entry);
            }
        }

        public ScopeEntry? FindLocal(string name)
        {
            if (_index.TryGetValue(name, out int idx))
                return _entries[idx];
            return null;
        }

        public bool HasLocal(string name) => _index.ContainsKey(name);
    }

    public class ScopeArena
    {
        private readonly List<Scope> _scopes = new List<Scope>();

        public ScopeId CreateScope(ScopeKind kind, ScopeId? parent)
        {
            var id = new ScopeId((uint)_scopes.Count);
            var scope = new Scope(id, kind, parent);
            _scopes.Add(scope);
            if (parent.HasValue)
                _scopes[(int)parent.Value.Value].Children.Add(id);
            return id;
        }

        public Scope Get(ScopeId id) => _scopes[(int)id.Value];

        public void AddEntry(ScopeId scopeId, ScopeEntry entry)
        {
            _scopes[(int)scopeId.Value].AddEntry(entry);
        }

        public WclValue? Resolve(ScopeId scopeId, string name)
        {
            var current = (ScopeId?)scopeId;
            while (current.HasValue)
            {
                var scope = _scopes[(int)current.Value.Value];
                var entry = scope.FindLocal(name);
                if (entry != null)
                {
                    entry.ReadCount++;
                    return entry.Value;
                }
                current = scope.Parent;
            }
            return null;
        }

        public ScopeEntry? ResolveEntry(ScopeId scopeId, string name)
        {
            var current = (ScopeId?)scopeId;
            while (current.HasValue)
            {
                var scope = _scopes[(int)current.Value.Value];
                var entry = scope.FindLocal(name);
                if (entry != null) return entry;
                current = scope.Parent;
            }
            return null;
        }

        public void SetEntryValue(ScopeId scopeId, string name, WclValue value)
        {
            var entry = _scopes[(int)scopeId.Value].FindLocal(name);
            if (entry != null)
            {
                entry.Value = value;
                entry.Evaluated = true;
            }
        }

        public bool HasEntry(ScopeId scopeId, string name)
        {
            var current = (ScopeId?)scopeId;
            while (current.HasValue)
            {
                var scope = _scopes[(int)current.Value.Value];
                if (scope.HasLocal(name)) return true;
                current = scope.Parent;
            }
            return false;
        }

        public Span? CheckShadowing(ScopeId scopeId, string name)
        {
            var scope = _scopes[(int)scopeId.Value];
            var current = scope.Parent;
            while (current.HasValue)
            {
                var parentScope = _scopes[(int)current.Value.Value];
                var entry = parentScope.FindLocal(name);
                if (entry != null) return entry.Span;
                current = parentScope.Parent;
            }
            return null;
        }

        /// <summary>
        /// Topological sort of entries within a scope using Kahn's algorithm.
        /// Returns entry names in evaluation order, or null with cycle participants on failure.
        /// </summary>
        public (List<string>? Order, List<string>? Cycle) TopoSort(ScopeId scopeId)
        {
            var scope = _scopes[(int)scopeId.Value];
            var entries = scope.Entries;
            if (entries.Count == 0)
                return (new List<string>(), null);

            var localNames = new HashSet<string>();
            foreach (var e in entries) localNames.Add(e.Name);

            // Build adjacency: name -> names it depends on (that are local)
            var inDegree = new Dictionary<string, int>();
            var dependents = new Dictionary<string, List<string>>(); // dep -> who depends on it

            foreach (var e in entries)
            {
                inDegree[e.Name] = 0;
                if (!dependents.ContainsKey(e.Name))
                    dependents[e.Name] = new List<string>();
            }

            foreach (var e in entries)
            {
                foreach (var dep in e.Dependencies)
                {
                    if (localNames.Contains(dep) && dep != e.Name)
                    {
                        inDegree[e.Name]++;
                        if (!dependents.ContainsKey(dep))
                            dependents[dep] = new List<string>();
                        dependents[dep].Add(e.Name);
                    }
                }
            }

            var queue = new Queue<string>();
            foreach (var kvp in inDegree)
                if (kvp.Value == 0)
                    queue.Enqueue(kvp.Key);

            var order = new List<string>();
            while (queue.Count > 0)
            {
                var name = queue.Dequeue();
                order.Add(name);
                if (dependents.TryGetValue(name, out var deps))
                {
                    foreach (var d in deps)
                    {
                        inDegree[d]--;
                        if (inDegree[d] == 0)
                            queue.Enqueue(d);
                    }
                }
            }

            if (order.Count < entries.Count)
            {
                var cycle = inDegree.Where(kvp => kvp.Value > 0).Select(kvp => kvp.Key).ToList();
                return (null, cycle);
            }

            return (order, null);
        }

        public IEnumerable<(ScopeId Id, ScopeEntry Entry)> AllEntries()
        {
            foreach (var scope in _scopes)
                foreach (var entry in scope.Entries)
                    yield return (scope.Id, entry);
        }

        public int Count => _scopes.Count;
    }
}
