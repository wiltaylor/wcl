using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using Wcl.Eval;
using Wcl.Native;
using Wcl.Serde;

namespace Wcl
{
    public static class WclParser
    {
        public static WclDocument Parse(string source, ParseOptions? options = null)
        {
            var optsJson = options?.ToJson();
            var sourcePtr = FfiHelper.ToUtf8(source);
            var optsPtr = FfiHelper.ToUtf8(optsJson);

            try
            {
                if (options?.Functions != null && options.Functions.Count > 0)
                {
                    return ParseWithFunctions(source, optsJson, options.Functions);
                }

                var handle = NativeMethods.wcl_ffi_parse(sourcePtr, optsPtr);
                if (handle == IntPtr.Zero)
                    throw new Exception("wcl: parse returned null");
                return new WclDocument(handle);
            }
            finally
            {
                FfiHelper.FreeUtf8(sourcePtr);
                FfiHelper.FreeUtf8(optsPtr);
            }
        }

        public static WclDocument ParseFile(string path, ParseOptions? options = null)
        {
            var optsJson = options?.ToJson();
            var pathPtr = FfiHelper.ToUtf8(path);
            var optsPtr = FfiHelper.ToUtf8(optsJson);

            try
            {
                var handle = NativeMethods.wcl_ffi_parse_file(pathPtr, optsPtr);
                if (handle == IntPtr.Zero)
                {
                    var errPtr = NativeMethods.wcl_ffi_last_error();
                    var errMsg = FfiHelper.ConsumeString(errPtr);
                    throw new Exception($"wcl: {(string.IsNullOrEmpty(errMsg) ? $"failed to parse file {path}" : errMsg)}");
                }
                return new WclDocument(handle);
            }
            finally
            {
                FfiHelper.FreeUtf8(pathPtr);
                FfiHelper.FreeUtf8(optsPtr);
            }
        }

        public static T FromString<T>(string source, ParseOptions? options = null)
        {
            using var doc = Parse(source, options);
            if (doc.HasErrors())
                throw new Exception("parse errors: " +
                    string.Join("; ", doc.Errors().ConvertAll(d => d.Message)));
            return WclDeserializer.FromValue<T>(WclValue.NewMap(doc.Values));
        }

        public static string ToString<T>(T value)
        {
            return WclSerializer.Serialize(value!, false);
        }

        public static string ToStringPretty<T>(T value)
        {
            return WclSerializer.Serialize(value!, true);
        }

        private static WclDocument ParseWithFunctions(string source, string? optsJson,
            Dictionary<string, Func<WclValue[], WclValue>> functions)
        {
            var count = functions.Count;
            var namePointers = new IntPtr[count];
            var callbackPointers = new IntPtr[count];
            var contextValues = new IntPtr[count];
            var callbackIds = new List<ulong>(count);

            int i = 0;
            foreach (var kvp in functions)
            {
                namePointers[i] = FfiHelper.ToUtf8(kvp.Key);
                var cbId = CallbackRegistry.Register(kvp.Value);
                callbackIds.Add(cbId);
                callbackPointers[i] = Marshal.GetFunctionPointerForDelegate(CallbackRegistry.TrampolineDelegate);
                contextValues[i] = new IntPtr((long)cbId);
                i++;
            }

            var sourcePtr = FfiHelper.ToUtf8(source);
            var optsPtr = FfiHelper.ToUtf8(optsJson);

            var namesHandle = GCHandle.Alloc(namePointers, GCHandleType.Pinned);
            var callbacksHandle = GCHandle.Alloc(callbackPointers, GCHandleType.Pinned);
            var contextsHandle = GCHandle.Alloc(contextValues, GCHandleType.Pinned);

            try
            {
                var handle = NativeMethods.wcl_ffi_parse_with_functions(
                    sourcePtr,
                    optsPtr,
                    namesHandle.AddrOfPinnedObject(),
                    callbacksHandle.AddrOfPinnedObject(),
                    contextsHandle.AddrOfPinnedObject(),
                    (UIntPtr)count);

                if (handle == IntPtr.Zero)
                {
                    foreach (var id in callbackIds)
                        CallbackRegistry.Unregister(id);
                    throw new Exception("wcl: parse returned null");
                }

                return new WclDocument(handle, callbackIds);
            }
            finally
            {
                namesHandle.Free();
                callbacksHandle.Free();
                contextsHandle.Free();

                for (int j = 0; j < count; j++)
                    FfiHelper.FreeUtf8(namePointers[j]);

                FfiHelper.FreeUtf8(sourcePtr);
                FfiHelper.FreeUtf8(optsPtr);
            }
        }
    }
}
