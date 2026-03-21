using System;
using System.Collections.Generic;
using System.Text.Json;
using Wcl.Eval;

namespace Wcl
{
    public class ParseOptions
    {
        public string? RootDir { get; set; }
        public bool? AllowImports { get; set; }
        public uint? MaxImportDepth { get; set; }
        public uint? MaxMacroDepth { get; set; }
        public uint? MaxLoopDepth { get; set; }
        public uint? MaxIterations { get; set; }
        public Dictionary<string, Func<WclValue[], WclValue>>? Functions { get; set; }
        public Dictionary<string, object>? Variables { get; set; }

        internal string? ToJson()
        {
            var parts = new List<string>();
            if (RootDir != null)
                parts.Add($"\"rootDir\":{JsonSerializer.Serialize(RootDir)}");
            if (AllowImports.HasValue)
                parts.Add($"\"allowImports\":{(AllowImports.Value ? "true" : "false")}");
            if (MaxImportDepth.HasValue)
                parts.Add($"\"maxImportDepth\":{MaxImportDepth.Value}");
            if (MaxMacroDepth.HasValue)
                parts.Add($"\"maxMacroDepth\":{MaxMacroDepth.Value}");
            if (MaxLoopDepth.HasValue)
                parts.Add($"\"maxLoopDepth\":{MaxLoopDepth.Value}");
            if (MaxIterations.HasValue)
                parts.Add($"\"maxIterations\":{MaxIterations.Value}");
            if (Variables != null && Variables.Count > 0)
                parts.Add($"\"variables\":{JsonSerializer.Serialize(Variables)}");
            if (parts.Count == 0)
                return null;
            return "{" + string.Join(",", parts) + "}";
        }
    }
}
