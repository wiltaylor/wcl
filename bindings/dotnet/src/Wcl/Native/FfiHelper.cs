using System;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace Wcl.Native
{
    internal static class FfiHelper
    {
        internal static IntPtr ToUtf8(string? s)
        {
            if (s == null) return IntPtr.Zero;
            return Marshal.StringToCoTaskMemUTF8(s);
        }

        internal static void FreeUtf8(IntPtr ptr)
        {
            if (ptr != IntPtr.Zero)
                Marshal.FreeCoTaskMem(ptr);
        }

        internal static string ConsumeString(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero) return "";
            var s = Marshal.PtrToStringUTF8(ptr) ?? "";
            NativeMethods.wcl_ffi_string_free(ptr);
            return s;
        }

        internal static JsonDocument ConsumeJsonDocument(IntPtr ptr)
        {
            var s = ConsumeString(ptr);
            return JsonDocument.Parse(s);
        }

        internal static (bool IsOk, JsonElement Value, string? Error) ConsumeJsonResult(IntPtr ptr)
        {
            var s = ConsumeString(ptr);
            using var doc = JsonDocument.Parse(s);
            if (doc.RootElement.TryGetProperty("error", out var errEl))
            {
                return (false, default, errEl.GetString());
            }
            if (doc.RootElement.TryGetProperty("ok", out var okEl))
            {
                return (true, okEl.Clone(), null);
            }
            return (false, default, "unexpected JSON result format");
        }
    }
}
