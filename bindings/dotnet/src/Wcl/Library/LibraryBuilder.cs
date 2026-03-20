using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Wcl.Native;

namespace Wcl.Library
{
    public class FunctionStub
    {
        public string Name { get; set; }
        public List<(string Name, string Type)> Params { get; set; }
        public string? ReturnType { get; set; }
        public string? Doc { get; set; }

        public FunctionStub(string name, List<(string, string)> parms, string? returnType = null, string? doc = null)
        {
            Name = name; Params = parms; ReturnType = returnType; Doc = doc;
        }

        public string ToWcl()
        {
            var paramStr = string.Join(", ", Params.Select(p => $"{p.Name}: {p.Type}"));
            var ret = ReturnType != null ? $" -> {ReturnType}" : "";
            return $"declare {Name}({paramStr}){ret}\n";
        }
    }

    public class LibraryBuilder
    {
        private readonly string _name;
        private readonly List<string> _schemaTexts = new List<string>();
        private readonly List<FunctionStub> _stubs = new List<FunctionStub>();

        public LibraryBuilder(string name) { _name = name; }

        public void AddSchemaText(string text) => _schemaTexts.Add(text);
        public void AddFunctionStub(FunctionStub stub) => _stubs.Add(stub);

        public string Build()
        {
            var sb = new StringBuilder();
            foreach (var schema in _schemaTexts)
            {
                sb.Append(schema);
                if (!schema.EndsWith("\n")) sb.AppendLine();
            }
            foreach (var stub in _stubs)
            {
                sb.Append(stub.ToWcl());
            }
            return sb.ToString();
        }

        public string Install()
        {
            return LibraryManager.Install(_name, Build());
        }
    }

    public static class LibraryManager
    {
        public static string Install(string name, string content)
        {
            var namePtr = FfiHelper.ToUtf8(name);
            var contentPtr = FfiHelper.ToUtf8(content);
            try
            {
                var resultPtr = NativeMethods.wcl_ffi_install_library(namePtr, contentPtr);
                var (isOk, value, error) = FfiHelper.ConsumeJsonResult(resultPtr);
                if (!isOk)
                    throw new Exception($"wcl: {error}");
                return value.GetString() ?? "";
            }
            finally
            {
                FfiHelper.FreeUtf8(namePtr);
                FfiHelper.FreeUtf8(contentPtr);
            }
        }

        public static void Uninstall(string name)
        {
            var namePtr = FfiHelper.ToUtf8(name);
            try
            {
                var resultPtr = NativeMethods.wcl_ffi_uninstall_library(namePtr);
                var (isOk, _, error) = FfiHelper.ConsumeJsonResult(resultPtr);
                if (!isOk)
                    throw new Exception($"wcl: {error}");
            }
            finally
            {
                FfiHelper.FreeUtf8(namePtr);
            }
        }

        public static List<string> List()
        {
            var resultPtr = NativeMethods.wcl_ffi_list_libraries();
            var (isOk, value, error) = FfiHelper.ConsumeJsonResult(resultPtr);
            if (!isOk)
                throw new Exception($"wcl: {error}");
            var result = new List<string>();
            foreach (var el in value.EnumerateArray())
                result.Add(el.GetString() ?? "");
            return result;
        }
    }
}
