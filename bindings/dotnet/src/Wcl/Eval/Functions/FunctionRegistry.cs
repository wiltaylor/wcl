using System;
using System.Collections.Generic;

namespace Wcl.Eval.Functions
{
    public class FunctionSignature
    {
        public string Name { get; set; }
        public List<string> Params { get; set; }
        public string ReturnType { get; set; }
        public string Doc { get; set; }

        public FunctionSignature(string name, List<string> parms, string returnType, string doc)
        {
            Name = name; Params = parms; ReturnType = returnType; Doc = doc;
        }
    }

    public class FunctionRegistry
    {
        public Dictionary<string, Func<WclValue[], WclValue>> Functions { get; }
            = new Dictionary<string, Func<WclValue[], WclValue>>();
        public List<FunctionSignature> Signatures { get; } = new List<FunctionSignature>();

        public void Register(string name, Func<WclValue[], WclValue> func, FunctionSignature? sig = null)
        {
            Functions[name] = func;
            if (sig != null) Signatures.Add(sig);
        }

        public WclValue? Call(string name, WclValue[] args)
        {
            if (Functions.TryGetValue(name, out var fn))
                return fn(args);
            return null;
        }
    }
}
