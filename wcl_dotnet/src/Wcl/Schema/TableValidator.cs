using System.Collections.Generic;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;

namespace Wcl.Schema
{
    public static class TableValidator
    {
        public static void ValidateTables(Document doc, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is TableItem ti)
                    ValidateTable(ti.Table, diags);
            }
        }

        private static void ValidateTable(Table table, DiagnosticBag diags)
        {
            var evaluator = new Evaluator();
            var scope = evaluator.Scopes.CreateScope(ScopeKind.Module, null);

            foreach (var row in table.Rows)
            {
                for (int i = 0; i < table.Columns.Count && i < row.Cells.Count; i++)
                {
                    try
                    {
                        var val = evaluator.EvalExpr(row.Cells[i], scope);
                        if (!TypeChecker.CheckType(val, table.Columns[i].TypeExpr))
                        {
                            diags.ErrorWithCode("E071",
                                $"table column '{table.Columns[i].Name.Name}': expected {TypeChecker.TypeName(table.Columns[i].TypeExpr)}, got {val.TypeName}",
                                row.Cells[i].GetSpan());
                        }
                    }
                    catch { }
                }
            }
        }
    }
}
