using Wcl.Eval.Functions;
using Wcl.Eval.Merge;

namespace Wcl
{
    public class ParseOptions
    {
        public string RootDir { get; set; } = ".";
        public uint MaxImportDepth { get; set; } = 32;
        public bool AllowImports { get; set; } = true;
        public ConflictMode MergeConflictMode { get; set; } = ConflictMode.Strict;
        public uint MaxMacroDepth { get; set; } = 64;
        public uint MaxLoopDepth { get; set; } = 32;
        public uint MaxIterations { get; set; } = 10_000;
        public FunctionRegistry Functions { get; set; } = new FunctionRegistry();
    }
}
