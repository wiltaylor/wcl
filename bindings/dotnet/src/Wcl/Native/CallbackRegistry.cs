using System;
using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using Wcl.Eval;

namespace Wcl.Native
{
    internal static class CallbackRegistry
    {
        private static long _nextId;
        private static readonly ConcurrentDictionary<ulong, Func<WclValue[], WclValue>> _callbacks
            = new ConcurrentDictionary<ulong, Func<WclValue[], WclValue>>();

        // Must be stored to prevent GC collection
        internal static readonly WclCallbackFn TrampolineDelegate = Trampoline;

        internal static ulong Register(Func<WclValue[], WclValue> fn)
        {
            var id = (ulong)Interlocked.Increment(ref _nextId);
            _callbacks[id] = fn;
            return id;
        }

        internal static void Unregister(ulong id)
        {
            _callbacks.TryRemove(id, out _);
        }

        // Allocate a UTF-8 C string using C malloc, matching what the Rust FFI expects
        // to free with libc::free. Marshal.StringToCoTaskMemUTF8 uses CoTaskMemAlloc on
        // Windows which is incompatible with libc::free.
        private static IntPtr AllocCString(string s)
        {
            var bytes = Encoding.UTF8.GetBytes(s);
            var ptr = Marshal.AllocHGlobal(bytes.Length + 1);
            Marshal.Copy(bytes, 0, ptr, bytes.Length);
            Marshal.WriteByte(ptr, bytes.Length, 0);
            return ptr;
        }

        [MonoPInvokeCallback(typeof(WclCallbackFn))]
        private static IntPtr Trampoline(IntPtr ctx, IntPtr argsJsonPtr)
        {
            var id = (ulong)ctx.ToInt64();
            if (!_callbacks.TryGetValue(id, out var fn))
            {
                return AllocCString("ERR:callback not found");
            }

            try
            {
                var argsJsonStr = Marshal.PtrToStringUTF8(argsJsonPtr) ?? "[]";
                using var doc = JsonDocument.Parse(argsJsonStr);
                var argsArray = doc.RootElement;

                var args = new WclValue[argsArray.GetArrayLength()];
                int i = 0;
                foreach (var el in argsArray.EnumerateArray())
                {
                    args[i++] = JsonConvert.ToWclValue(el);
                }

                var result = fn(args);
                var resultJson = JsonConvert.WclValueToJson(result);
                return AllocCString(resultJson);
            }
            catch (Exception ex)
            {
                return AllocCString("ERR:" + ex.Message);
            }
        }
    }

    [AttributeUsage(AttributeTargets.Method)]
    internal sealed class MonoPInvokeCallbackAttribute : Attribute
    {
        public MonoPInvokeCallbackAttribute(Type delegateType) { }
    }
}
