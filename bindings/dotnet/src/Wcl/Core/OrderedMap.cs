using System;
using System.Collections;
using System.Collections.Generic;

namespace Wcl.Core
{
    public class OrderedMap<TKey, TValue> : IEnumerable<KeyValuePair<TKey, TValue>>
        where TKey : notnull
    {
        private readonly List<KeyValuePair<TKey, TValue>> _entries = new List<KeyValuePair<TKey, TValue>>();
        private readonly Dictionary<TKey, int> _index = new Dictionary<TKey, int>();

        public int Count => _entries.Count;

        public TValue this[TKey key]
        {
            get => _entries[_index[key]].Value;
            set
            {
                if (_index.TryGetValue(key, out int idx))
                {
                    _entries[idx] = new KeyValuePair<TKey, TValue>(key, value);
                }
                else
                {
                    _index[key] = _entries.Count;
                    _entries.Add(new KeyValuePair<TKey, TValue>(key, value));
                }
            }
        }

        public void Add(TKey key, TValue value)
        {
            if (_index.ContainsKey(key))
                throw new ArgumentException($"Key '{key}' already exists");
            _index[key] = _entries.Count;
            _entries.Add(new KeyValuePair<TKey, TValue>(key, value));
        }

        public bool TryGetValue(TKey key, out TValue value)
        {
            if (_index.TryGetValue(key, out int idx))
            {
                value = _entries[idx].Value!;
                return true;
            }
            value = default!;
            return false;
        }

        public bool ContainsKey(TKey key) => _index.ContainsKey(key);

        public bool Remove(TKey key)
        {
            if (!_index.TryGetValue(key, out int idx))
                return false;

            _entries.RemoveAt(idx);
            _index.Remove(key);

            // Update indices for entries after the removed one
            for (int i = idx; i < _entries.Count; i++)
                _index[_entries[i].Key] = i;

            return true;
        }

        public IReadOnlyList<TKey> Keys
        {
            get
            {
                var keys = new List<TKey>(_entries.Count);
                foreach (var e in _entries) keys.Add(e.Key);
                return keys;
            }
        }

        public IReadOnlyList<TValue> Values
        {
            get
            {
                var vals = new List<TValue>(_entries.Count);
                foreach (var e in _entries) vals.Add(e.Value);
                return vals;
            }
        }

        public KeyValuePair<TKey, TValue> GetAt(int index) => _entries[index];

        public void Insert(TKey key, TValue value)
        {
            if (_index.TryGetValue(key, out int idx))
            {
                _entries[idx] = new KeyValuePair<TKey, TValue>(key, value);
            }
            else
            {
                _index[key] = _entries.Count;
                _entries.Add(new KeyValuePair<TKey, TValue>(key, value));
            }
        }

        public void Clear()
        {
            _entries.Clear();
            _index.Clear();
        }

        public IEnumerator<KeyValuePair<TKey, TValue>> GetEnumerator() => _entries.GetEnumerator();
        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }
}
