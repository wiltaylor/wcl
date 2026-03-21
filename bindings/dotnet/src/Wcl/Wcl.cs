using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using Wcl.Eval;
using Wcl.Wasm;
using Wcl.Serde;

namespace Wcl
{
    public static class WclParser
    {
        public static WclDocument Parse(string source, ParseOptions? options = null)
        {
            var optsJson = options?.ToJson();

            if (options?.Functions != null && options.Functions.Count > 0)
            {
                return ParseWithFunctions(source, optsJson, options.Functions);
            }

            var handle = WasmRuntime.Instance.Parse(source, optsJson);
            if (handle == 0)
                throw new Exception("wcl: parse returned invalid handle");
            return new WclDocument(handle);
        }

        public static WclDocument ParseFile(string path, ParseOptions? options = null)
        {
            var source = File.ReadAllText(path);

            // Set rootDir from file path if not specified
            var opts = options ?? new ParseOptions();
            if (opts.RootDir == null)
            {
                var dir = Path.GetDirectoryName(Path.GetFullPath(path));
                if (dir != null)
                    opts.RootDir = dir;
            }

            return Parse(source, opts);
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
            var funcNames = new List<string>(functions.Keys);
            var funcNamesJson = JsonSerializer.Serialize(funcNames);

            WasmCallbackBridge.SetFunctions(functions);
            try
            {
                var handle = WasmRuntime.Instance.ParseWithFunctions(source, optsJson, funcNamesJson);
                if (handle == 0)
                    throw new Exception("wcl: parse returned invalid handle");
                return new WclDocument(handle);
            }
            finally
            {
                WasmCallbackBridge.ClearFunctions();
            }
        }
    }
}
