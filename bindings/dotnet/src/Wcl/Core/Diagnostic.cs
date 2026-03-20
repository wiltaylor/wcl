namespace Wcl.Core
{
    public class Diagnostic
    {
        public string Severity { get; }
        public string Message { get; }
        public string? Code { get; }

        public Diagnostic(string severity, string message, string? code = null)
        {
            Severity = severity;
            Message = message;
            Code = code;
        }

        public bool IsError => Severity == "error";

        public override string ToString() =>
            Code != null ? $"[{Code}] {Severity}: {Message}" : $"{Severity}: {Message}";
    }
}
