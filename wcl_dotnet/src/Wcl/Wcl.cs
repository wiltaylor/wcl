using System.Collections.Generic;
using System.IO;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;
using Wcl.Eval.ControlFlow;
using Wcl.Eval.Import;
using Wcl.Eval.Macros;
using Wcl.Eval.Merge;
using Wcl.Schema;
using Wcl.Serde;

namespace Wcl
{
    public static class WclParser
    {
        /// <summary>
        /// Parse a WCL document through the full 11-phase pipeline.
        /// </summary>
        public static WclDocument Parse(string source, ParseOptions? options = null)
        {
            options ??= new ParseOptions();
            var sourceMap = new SourceMap();
            var fileId = sourceMap.AddFile("<input>", source);
            var allDiagnostics = new List<Diagnostic>();

            // Phase 1: Parse
            var (doc, parseDiags) = Core.Parser.WclParser.Parse(source, fileId);
            allDiagnostics.AddRange(parseDiags.IntoDiagnostics());

            // Phase 2: Macro collection
            var macroRegistry = new MacroRegistry();
            var diagBag = new DiagnosticBag();
            macroRegistry.Collect(doc, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            // Phase 3: Import resolution
            if (options.AllowImports)
            {
                var fs = new RealFileSystem();
                var resolver = new ImportResolver(fs, sourceMap, options.RootDir,
                    options.MaxImportDepth, options.AllowImports);
                var importDiags = resolver.Resolve(doc,
                    Path.Combine(options.RootDir, "<input>"), 0);
                allDiagnostics.AddRange(importDiags.IntoDiagnostics());
            }

            // Phase 4: Macro expansion
            var macroExpander = new MacroExpander(macroRegistry, options.MaxMacroDepth);
            macroExpander.Expand(doc);
            allDiagnostics.AddRange(macroExpander.IntoDiagnostics().IntoDiagnostics());

            // Phase 5: Control flow expansion
            var cfExpander = new ControlFlowExpander(options.MaxLoopDepth, options.MaxIterations);
            var preEval = new Evaluator(options.Functions);
            var preScope = preEval.Scopes.CreateScope(ScopeKind.Module, null);

            // Pre-register let bindings for control flow
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is LetBindingItem lbi)
                {
                    try
                    {
                        var val = preEval.EvalExpr(lbi.LetBinding.Value, preScope);
                        var entry = new ScopeEntry(
                            lbi.LetBinding.Name.Name, ScopeEntryKind.LetBinding, val, lbi.LetBinding.Span);
                        entry.Evaluated = true;
                        preEval.Scopes.AddEntry(preScope, entry);
                    }
                    catch { }
                }
            }

            cfExpander.Expand(doc, expr =>
            {
                return preEval.EvalExpr(expr, preScope);
            });
            allDiagnostics.AddRange(cfExpander.IntoDiagnostics().IntoDiagnostics());

            // Phase 6: Partial merge
            var merger = new PartialMerger(options.MergeConflictMode);
            merger.Merge(doc);
            allDiagnostics.AddRange(merger.IntoDiagnostics().IntoDiagnostics());

            // Phase 7: Scope construction + evaluation
            var evaluator = new Evaluator(options.Functions);
            var values = evaluator.Evaluate(doc);
            allDiagnostics.AddRange(evaluator.IntoDiagnostics().IntoDiagnostics());

            // Phase 8: Decorator validation
            var decoratorSchemas = new DecoratorSchemaRegistry();
            diagBag = new DiagnosticBag();
            decoratorSchemas.Collect(doc, diagBag);
            decoratorSchemas.ValidateAll(doc, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            // Phase 9: Schema validation
            var schemas = new SchemaRegistry();
            diagBag = new DiagnosticBag();
            schemas.Collect(doc, diagBag);
            schemas.Validate(doc, values, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            // Phase 9b: Table validation
            diagBag = new DiagnosticBag();
            TableValidator.ValidateTables(doc, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            // Phase 10: ID uniqueness
            var idRegistry = new IdRegistry();
            diagBag = new DiagnosticBag();
            idRegistry.CheckDocument(doc, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            // Phase 11: Document validation (uses fresh evaluator with custom functions,
            // matching Rust which passes a fresh Evaluator::with_functions)
            diagBag = new DiagnosticBag();
            var valEval = Evaluator.WithFunctions(options.Functions);
            DocumentValidator.ValidateDocument(doc, valEval, diagBag);
            allDiagnostics.AddRange(diagBag.IntoDiagnostics());

            return new WclDocument(doc, values, allDiagnostics, sourceMap,
                schemas, decoratorSchemas);
        }

        /// <summary>
        /// Parse WCL and deserialize into a C# type.
        /// </summary>
        public static T FromString<T>(string source, ParseOptions? options = null)
        {
            var doc = Parse(source, options);
            if (doc.HasErrors())
                throw new System.Exception("parse errors: " +
                    string.Join("; ", doc.Errors().ConvertAll(d => d.Message)));
            return WclDeserializer.FromValue<T>(WclValue.NewMap(doc.Values));
        }

        /// <summary>
        /// Serialize a C# object to WCL text.
        /// </summary>
        public static string ToString<T>(T value)
        {
            return WclSerializer.Serialize(value!, false);
        }

        /// <summary>
        /// Serialize a C# object to pretty-printed WCL text.
        /// </summary>
        public static string ToStringPretty<T>(T value)
        {
            return WclSerializer.Serialize(value!, true);
        }
    }
}
