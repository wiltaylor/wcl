using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Wcl.Core;
using Wcl.Eval;
using Wcl.Native;

namespace Wcl
{
    public class WclDocument : IDisposable
    {
        private IntPtr _handle;
        private readonly List<ulong> _callbackIds;
        private bool _disposed;
        private readonly object _lock = new object();

        private OrderedMap<string, WclValue>? _cachedValues;
        private List<Diagnostic>? _cachedDiagnostics;

        internal WclDocument(IntPtr handle, List<ulong>? callbackIds = null)
        {
            _handle = handle;
            _callbackIds = callbackIds ?? new List<ulong>();
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
                        var ptr = NativeMethods.wcl_ffi_document_values(_handle);
                        var json = FfiHelper.ConsumeString(ptr);
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
                        var ptr = NativeMethods.wcl_ffi_document_diagnostics(_handle);
                        var json = FfiHelper.ConsumeString(ptr);
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
                return NativeMethods.wcl_ffi_document_has_errors(_handle);
            }
        }

        public List<Diagnostic> Errors() => Diagnostics.Where(d => d.IsError).ToList();

        public WclValue Query(string query)
        {
            lock (_lock)
            {
                CheckDisposed();
                var queryPtr = FfiHelper.ToUtf8(query);
                try
                {
                    var resultPtr = NativeMethods.wcl_ffi_document_query(_handle, queryPtr);
                    var (isOk, value, error) = FfiHelper.ConsumeJsonResult(resultPtr);
                    if (!isOk)
                        throw new Exception($"query error: {error}");
                    return JsonConvert.ToWclValue(value);
                }
                finally
                {
                    FfiHelper.FreeUtf8(queryPtr);
                }
            }
        }

        public List<BlockRef> Blocks()
        {
            lock (_lock)
            {
                CheckDisposed();
                var ptr = NativeMethods.wcl_ffi_document_blocks(_handle);
                var json = FfiHelper.ConsumeString(ptr);
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
                var kindPtr = FfiHelper.ToUtf8(kind);
                try
                {
                    var ptr = NativeMethods.wcl_ffi_document_blocks_of_type(_handle, kindPtr);
                    var json = FfiHelper.ConsumeString(ptr);
                    using var doc = JsonDocument.Parse(json);
                    var result = new List<BlockRef>();
                    foreach (var el in doc.RootElement.EnumerateArray())
                        result.Add(JsonConvert.ToBlockRef(el));
                    return result;
                }
                finally
                {
                    FfiHelper.FreeUtf8(kindPtr);
                }
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

                if (_handle != IntPtr.Zero)
                {
                    NativeMethods.wcl_ffi_document_free(_handle);
                    _handle = IntPtr.Zero;
                }

                foreach (var id in _callbackIds)
                    CallbackRegistry.Unregister(id);
                _callbackIds.Clear();
            }
            GC.SuppressFinalize(this);
        }

        ~WclDocument()
        {
            Dispose();
        }
    }
}
