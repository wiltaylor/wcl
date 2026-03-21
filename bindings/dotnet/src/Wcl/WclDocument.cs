using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Wcl.Core;
using Wcl.Eval;
using Wcl.Wasm;

namespace Wcl
{
    public class WclDocument : IDisposable
    {
        private int _handle;
        private bool _disposed;
        private readonly object _lock = new object();

        private OrderedMap<string, WclValue>? _cachedValues;
        private List<Diagnostic>? _cachedDiagnostics;

        internal WclDocument(int handle)
        {
            _handle = handle;
        }

        public OrderedMap<string, WclValue> Values
        {
            get
            {
                lock (_lock)
                {
                    CheckDisposed();
                    if (_cachedValues == null)
                    {
                        var json = WasmRuntime.Instance.DocumentValues(_handle);
                        using var doc = JsonDocument.Parse(json);
                        _cachedValues = JsonConvert.ToValues(doc.RootElement);
                    }
                    return _cachedValues;
                }
            }
        }

        public List<Diagnostic> Diagnostics
        {
            get
            {
                lock (_lock)
                {
                    CheckDisposed();
                    if (_cachedDiagnostics == null)
                    {
                        var json = WasmRuntime.Instance.DocumentDiagnostics(_handle);
                        using var doc = JsonDocument.Parse(json);
                        _cachedDiagnostics = new List<Diagnostic>();
                        foreach (var el in doc.RootElement.EnumerateArray())
                            _cachedDiagnostics.Add(JsonConvert.ToDiagnostic(el));
                    }
                    return _cachedDiagnostics;
                }
            }
        }

        public bool HasErrors()
        {
            lock (_lock)
            {
                CheckDisposed();
                return WasmRuntime.Instance.DocumentHasErrors(_handle);
            }
        }

        public List<Diagnostic> Errors() => Diagnostics.Where(d => d.IsError).ToList();

        public WclValue Query(string query)
        {
            lock (_lock)
            {
                CheckDisposed();
                var resultJson = WasmRuntime.Instance.DocumentQuery(_handle, query);
                using var doc = JsonDocument.Parse(resultJson);
                if (doc.RootElement.TryGetProperty("error", out var errEl))
                    throw new Exception($"query error: {errEl.GetString()}");
                if (doc.RootElement.TryGetProperty("ok", out var okEl))
                    return JsonConvert.ToWclValue(okEl);
                throw new Exception("unexpected query result format");
            }
        }

        public List<BlockRef> Blocks()
        {
            lock (_lock)
            {
                CheckDisposed();
                var json = WasmRuntime.Instance.DocumentBlocks(_handle);
                using var doc = JsonDocument.Parse(json);
                var result = new List<BlockRef>();
                foreach (var el in doc.RootElement.EnumerateArray())
                    result.Add(JsonConvert.ToBlockRef(el));
                return result;
            }
        }

        public List<BlockRef> BlocksOfType(string kind)
        {
            lock (_lock)
            {
                CheckDisposed();
                var json = WasmRuntime.Instance.DocumentBlocksOfType(_handle, kind);
                using var doc = JsonDocument.Parse(json);
                var result = new List<BlockRef>();
                foreach (var el in doc.RootElement.EnumerateArray())
                    result.Add(JsonConvert.ToBlockRef(el));
                return result;
            }
        }

        private void CheckDisposed()
        {
            if (_disposed)
                throw new ObjectDisposedException(nameof(WclDocument));
        }

        public void Dispose()
        {
            lock (_lock)
            {
                if (_disposed) return;
                _disposed = true;

                if (_handle != 0)
                {
                    WasmRuntime.Instance.DocumentFree(_handle);
                    _handle = 0;
                }
            }
            GC.SuppressFinalize(this);
        }

        ~WclDocument()
        {
            Dispose();
        }
    }
}
