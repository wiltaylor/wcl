using System.Collections.Generic;

namespace Wcl.Core
{
    public enum Severity
    {
        Error,
        Warning
    }

    public class Label
    {
        public Span Span { get; }
        public string Message { get; }

        public Label(Span span, string message)
        {
            Span = span;
            Message = message;
        }
    }

    public class Diagnostic
    {
        public Severity Severity { get; }
        public string Message { get; }
        public Span Span { get; }
        public List<Label> Labels { get; }
        public List<string> Notes { get; }
        public string? Code { get; private set; }

        private Diagnostic(Severity severity, string message, Span span)
        {
            Severity = severity;
            Message = message;
            Span = span;
            Labels = new List<Label>();
            Notes = new List<string>();
        }

        public static Diagnostic Error(string message, Span span) =>
            new Diagnostic(Severity.Error, message, span);

        public static Diagnostic Warning(string message, Span span) =>
            new Diagnostic(Severity.Warning, message, span);

        public Diagnostic WithCode(string code)
        {
            Code = code;
            return this;
        }

        public Diagnostic WithLabel(Span span, string message)
        {
            Labels.Add(new Label(span, message));
            return this;
        }

        public Diagnostic WithNote(string note)
        {
            Notes.Add(note);
            return this;
        }

        public bool IsError => Severity == Severity.Error;

        public override string ToString() =>
            Code != null ? $"[{Code}] {Severity}: {Message}" : $"{Severity}: {Message}";
    }
}
