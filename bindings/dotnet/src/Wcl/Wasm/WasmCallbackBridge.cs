using System;
using System.Collections.Generic;
using System.Text.Json;
using Wcl.Eval;

namespace Wcl.Wasm
{
    internal static class WasmCallbackBridge
    {
        [ThreadStatic]
        private static Dictionary<string, Func<WclValue[], WclValue>>? _functions;

        internal static void SetFunctions(Dictionary<string, Func<WclValue[], WclValue>> functions)
        {
            _functions = functions;
        }

        internal static void ClearFunctions()
        {
            _functions = null;
        }

        internal static (bool Success, string? ResultJson) Invoke(string name, string argsJson)
        {
            if (_functions == null || !_functions.TryGetValue(name, out var fn))
            {
                return (false, $"callback not found: {name}");
            }

            try
            {
                using var doc = JsonDocument.Parse(argsJson);
                var argsArray = doc.RootElement;

                var args = new WclValue[argsArray.GetArrayLength()];
                int i = 0;
                foreach (var el in argsArray.EnumerateArray())
                {
                    args[i++] = JsonConvert.ToWclValue(el);
                }

                var result = fn(args);
                var resultJson = JsonConvert.WclValueToJson(result);
                return (true, resultJson);
            }
            catch (Exception ex)
            {
                return (false, ex.Message);
            }
        }
    }
}
