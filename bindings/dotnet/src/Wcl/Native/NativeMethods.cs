using System;
using System.Runtime.InteropServices;

namespace Wcl.Native
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate IntPtr WclCallbackFn(IntPtr ctx, IntPtr argsJson);

    internal static class NativeMethods
    {
        private const string LibName = "wcl_ffi";

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_parse(IntPtr source, IntPtr optionsJson);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_parse_file(IntPtr path, IntPtr optionsJson);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_last_error();

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void wcl_ffi_document_free(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_values(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.U1)]
        internal static extern bool wcl_ffi_document_has_errors(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_errors(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_diagnostics(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_query(IntPtr doc, IntPtr query);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_blocks(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_document_blocks_of_type(IntPtr doc, IntPtr kind);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_parse_with_functions(
            IntPtr source,
            IntPtr optionsJson,
            IntPtr funcNames,
            IntPtr funcCallbacks,
            IntPtr funcContexts,
            UIntPtr funcCount);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_install_library(IntPtr name, IntPtr content);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_uninstall_library(IntPtr name);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr wcl_ffi_list_libraries();

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void wcl_ffi_string_free(IntPtr s);
    }
}
