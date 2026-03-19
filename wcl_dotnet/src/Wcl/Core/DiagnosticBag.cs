using System.Collections.Generic;

namespace Wcl.Core
{
    public class DiagnosticBag
    {
        private readonly List<Diagnostic> _diagnostics = new List<Diagnostic>();

        public void Add(Diagnostic diagnostic) => _diagnostics.Add(diagnostic);

        public void Error(string message, Span span) =>
            _diagnostics.Add(Diagnostic.Error(message, span));

        public Diagnostic ErrorWithCode(string code, string message, Span span)
        {
            var d = Diagnostic.Error(message, span).WithCode(code);
            _diagnostics.Add(d);
            return d;
        }

        public void Warning(string message, Span span) =>
            _diagnostics.Add(Diagnostic.Warning(message, span));

        public Diagnostic WarningWithCode(string code, string message, Span span)
        {
            var d = Diagnostic.Warning(message, span).WithCode(code);
            _diagnostics.Add(d);
            return d;
        }

        public bool HasErrors
        {
            get
            {
                foreach (var d in _diagnostics)
                    if (d.IsError) return true;
                return false;
            }
        }

        public void Merge(DiagnosticBag other) => _diagnostics.AddRange(other._diagnostics);

        public List<Diagnostic> IntoDiagnostics() => _diagnostics;

        public int Count => _diagnostics.Count;
    }
}
