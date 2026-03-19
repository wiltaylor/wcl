using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;

namespace Wcl.Tests.Helpers
{
    public static class TestHelpers
    {
        public static Span DummySpan() => Span.Dummy();

        public static Ident MakeIdent(string name) => new Ident(name, Span.Dummy());

        public static WclDocument ParseDoc(string source, ParseOptions? options = null)
        {
            return WclParser.Parse(source, options);
        }

        public static WclValue? EvalAttr(string source, string attrName)
        {
            var doc = ParseDoc(source);
            doc.Values.TryGetValue(attrName, out var val);
            return val;
        }

        public static WclValue Eval(string exprSource)
        {
            var doc = ParseDoc($"__result = {exprSource}");
            if (doc.Values.TryGetValue("__result", out var val))
                return val;
            throw new System.Exception($"evaluation failed: {string.Join("; ", doc.Errors().ConvertAll(d => d.Message))}");
        }
    }
}
