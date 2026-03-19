using System.Collections.Generic;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;

namespace Wcl.Schema
{
    public static class DocumentValidator
    {
        public static void ValidateDocument(Document doc, Evaluator evaluator, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is ValidationItem vi)
                {
                    ValidateValidationBlock(vi.Validation, evaluator, diags);
                }
            }
        }

        private static void ValidateValidationBlock(Validation validation, Evaluator evaluator, DiagnosticBag diags)
        {
            var scope = evaluator.Scopes.CreateScope(ScopeKind.Block, null);

            // Evaluate let bindings
            foreach (var let in validation.Lets)
            {
                try
                {
                    var val = evaluator.EvalExpr(let.Value, scope);
                    evaluator.Scopes.AddEntry(scope, new ScopeEntry(
                        let.Name.Name, ScopeEntryKind.LetBinding, val, let.Span));
                }
                catch { }
            }

            // Evaluate check expression
            try
            {
                var checkVal = evaluator.EvalExpr(validation.Check, scope);
                if (checkVal.IsTruthy() == false)
                {
                    // Get message
                    string message;
                    try
                    {
                        var msgVal = evaluator.EvalExpr(validation.Message, scope);
                        message = msgVal.ToInterpString();
                    }
                    catch
                    {
                        message = "validation failed";
                    }

                    var validationName = GetStringLitValue(validation.Name);
                    bool isWarning = validation.Decorators.Exists(d => d.Name.Name == "warning");

                    if (isWarning)
                    {
                        diags.WarningWithCode("E080",
                            $"validation '{validationName}' failed: {message}",
                            validation.Span);
                    }
                    else
                    {
                        diags.ErrorWithCode("E080",
                            $"validation '{validationName}' failed: {message}",
                            validation.Span);
                    }
                }
            }
            catch { }
        }

        private static string GetStringLitValue(StringLit sl)
        {
            if (sl.Parts.Count == 1 && sl.Parts[0] is LiteralPart lp) return lp.Value;
            var sb = new System.Text.StringBuilder();
            foreach (var part in sl.Parts)
                if (part is LiteralPart lp2) sb.Append(lp2.Value);
            return sb.ToString();
        }
    }
}
