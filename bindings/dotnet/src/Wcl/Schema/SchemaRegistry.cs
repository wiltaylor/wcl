using System.Collections.Generic;
using System.Linq;
using System.Text.RegularExpressions;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval;

namespace Wcl.Schema
{
    public class ResolvedField
    {
        public string Name { get; set; }
        public TypeExpr TypeExpr { get; set; }
        public bool Optional { get; set; }
        public double? Min { get; set; }
        public double? Max { get; set; }
        public string? Pattern { get; set; }
        public List<string>? OneOf { get; set; }
        public string? RefSchema { get; set; }
        public string? IdPattern { get; set; }

        public ResolvedField(string name, TypeExpr typeExpr)
        {
            Name = name; TypeExpr = typeExpr;
        }
    }

    public class ResolvedSchema
    {
        public string Name { get; set; }
        public List<ResolvedField> Fields { get; set; }
        public bool Closed { get; set; }

        public ResolvedSchema(string name, List<ResolvedField> fields, bool closed)
        {
            Name = name; Fields = fields; Closed = closed;
        }
    }

    public class SchemaRegistry
    {
        private readonly Dictionary<string, ResolvedSchema> _schemas = new Dictionary<string, ResolvedSchema>();

        public void Collect(Document doc, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is SchemaItem si)
                {
                    var name = GetStringLitValue(si.Schema.Name);
                    if (_schemas.ContainsKey(name))
                    {
                        diags.ErrorWithCode("E001", $"duplicate schema name: '{name}'", si.Schema.Span);
                        continue;
                    }

                    bool closed = si.Schema.Decorators.Any(d => d.Name.Name == "closed");
                    var fields = new List<ResolvedField>();
                    foreach (var field in si.Schema.Fields)
                    {
                        var rf = new ResolvedField(field.Name.Name, field.TypeExpr);
                        var allDecorators = field.DecoratorsBefore.Concat(field.DecoratorsAfter);
                        foreach (var dec in allDecorators)
                        {
                            switch (dec.Name.Name)
                            {
                                case "optional": rf.Optional = true; break;
                                case "min":
                                    if (dec.Args.Count > 0 && dec.Args[0] is PositionalDecoratorArg minArg)
                                        if (minArg.Value is IntLitExpr minInt) rf.Min = minInt.Value;
                                        else if (minArg.Value is FloatLitExpr minFloat) rf.Min = minFloat.Value;
                                    break;
                                case "max":
                                    if (dec.Args.Count > 0 && dec.Args[0] is PositionalDecoratorArg maxArg)
                                        if (maxArg.Value is IntLitExpr maxInt) rf.Max = maxInt.Value;
                                        else if (maxArg.Value is FloatLitExpr maxFloat) rf.Max = maxFloat.Value;
                                    break;
                                case "pattern":
                                    if (dec.Args.Count > 0 && dec.Args[0] is PositionalDecoratorArg patArg)
                                        if (patArg.Value is StringLitExpr patStr)
                                            rf.Pattern = GetStringLitValue(patStr.StringLit);
                                    break;
                                case "one_of":
                                    rf.OneOf = new List<string>();
                                    foreach (var arg in dec.Args)
                                        if (arg is PositionalDecoratorArg pa && pa.Value is StringLitExpr se)
                                            rf.OneOf.Add(GetStringLitValue(se.StringLit));
                                    break;
                                case "ref":
                                    if (dec.Args.Count > 0 && dec.Args[0] is PositionalDecoratorArg refArg)
                                        if (refArg.Value is StringLitExpr refStr)
                                            rf.RefSchema = GetStringLitValue(refStr.StringLit);
                                    break;
                                case "id_pattern":
                                    if (dec.Args.Count > 0 && dec.Args[0] is PositionalDecoratorArg idArg)
                                        if (idArg.Value is StringLitExpr idStr)
                                            rf.IdPattern = GetStringLitValue(idStr.StringLit);
                                    break;
                            }
                        }
                        fields.Add(rf);
                    }
                    _schemas[name] = new ResolvedSchema(name, fields, closed);
                }
            }
        }

        public void Validate(Document doc, OrderedMap<string, WclValue> values, DiagnosticBag diags)
        {
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi && bdi.BodyItem is BlockItem bi)
                {
                    // Check if block kind matches any schema
                    ValidateBlock(bi.Block, values, doc, diags);
                }
            }
        }

        private void ValidateBlock(Block block, OrderedMap<string, WclValue> values, Document doc, DiagnosticBag diags)
        {
            // Look for schema decorator or matching schema name
            var schemaName = block.Decorators.FirstOrDefault(d => d.Name.Name == "schema")
                ?.Args.OfType<PositionalDecoratorArg>().FirstOrDefault()
                ?.Value;

            string? sName = null;
            if (schemaName is StringLitExpr sle)
                sName = GetStringLitValue(sle.StringLit);

            if (sName == null) sName = block.Kind.Name;

            if (!_schemas.TryGetValue(sName, out var schema)) return;

            // Resolve block values from the pre-evaluated values map
            var attrNames = new HashSet<string>();
            var attrValues = new Dictionary<string, (WclValue Value, Span Span)>();

            // Try to find the evaluated block in the values map
            WclValue? blockValue = null;
            if (values.TryGetValue(block.Kind.Name, out var bv))
                blockValue = bv;

            foreach (var bodyItem in block.Body)
            {
                if (bodyItem is AttributeItem ai)
                {
                    attrNames.Add(ai.Attribute.Name.Name);

                    // Try to get value from evaluated BlockRef first
                    WclValue? val = null;
                    if (blockValue != null)
                    {
                        if (blockValue.Kind == WclValueKind.BlockRef)
                            blockValue.AsBlockRef().Attributes.TryGetValue(ai.Attribute.Name.Name, out val);
                        else if (blockValue.Kind == WclValueKind.List)
                        {
                            // Multiple blocks - try each
                            foreach (var item in blockValue.AsList())
                                if (item.Kind == WclValueKind.BlockRef &&
                                    item.AsBlockRef().Attributes.TryGetValue(ai.Attribute.Name.Name, out val))
                                    break;
                        }
                    }

                    // Fall back to fresh evaluation if not found
                    if (val == null)
                    {
                        var evaluator = new Evaluator();
                        var scope = evaluator.Scopes.CreateScope(ScopeKind.Module, null);
                        try { val = evaluator.EvalExpr(ai.Attribute.Value, scope); }
                        catch { }
                    }

                    if (val != null)
                        attrValues[ai.Attribute.Name.Name] = (val, ai.Attribute.Span);
                }
            }

            // E070: Required field check
            foreach (var field in schema.Fields)
            {
                if (!field.Optional && !attrNames.Contains(field.Name))
                {
                    diags.ErrorWithCode("E070",
                        $"missing required field '{field.Name}' (schema '{sName}')",
                        block.Span);
                }
            }

            // E072: Unknown attribute in closed schema
            if (schema.Closed)
            {
                var fieldNames = new HashSet<string>(schema.Fields.Select(f => f.Name));
                foreach (var name in attrNames)
                {
                    if (!fieldNames.Contains(name))
                    {
                        diags.ErrorWithCode("E072",
                            $"unknown attribute '{name}' in closed schema '{sName}'",
                            block.Span);
                    }
                }
            }

            // Validate field constraints
            foreach (var field in schema.Fields)
            {
                if (!attrValues.TryGetValue(field.Name, out var entry)) continue;
                var (val, span) = entry;

                // E071: Type check
                if (!TypeChecker.CheckType(val, field.TypeExpr))
                {
                    diags.ErrorWithCode("E071",
                        $"type mismatch for '{field.Name}': expected {TypeChecker.TypeName(field.TypeExpr)}, got {val.TypeName}",
                        span);
                }

                // E073: Min/max
                if (field.Min.HasValue || field.Max.HasValue)
                {
                    double? numVal = val.Kind == WclValueKind.Int ? val.AsInt() :
                                     val.Kind == WclValueKind.Float ? val.AsFloat() : (double?)null;
                    if (numVal.HasValue)
                    {
                        if (field.Min.HasValue && numVal.Value < field.Min.Value)
                            diags.ErrorWithCode("E073", $"'{field.Name}' value {numVal} is below minimum {field.Min}", span);
                        if (field.Max.HasValue && numVal.Value > field.Max.Value)
                            diags.ErrorWithCode("E073", $"'{field.Name}' value {numVal} exceeds maximum {field.Max}", span);
                    }
                }

                // E074: Pattern
                if (field.Pattern != null && val.Kind == WclValueKind.String)
                {
                    if (!Regex.IsMatch(val.AsString(), field.Pattern))
                        diags.ErrorWithCode("E074", $"'{field.Name}' does not match pattern '{field.Pattern}'", span);
                }

                // E075: OneOf
                if (field.OneOf != null && val.Kind == WclValueKind.String)
                {
                    if (!field.OneOf.Contains(val.AsString()))
                        diags.ErrorWithCode("E075", $"'{field.Name}' value must be one of: {string.Join(", ", field.OneOf)}", span);
                }

                // E076: @ref target validation
                if (field.RefSchema != null && val.Kind == WclValueKind.Identifier)
                {
                    // Check that the referenced ID exists in blocks of the target schema type
                    var refId = val.AsIdentifier();
                    bool found = false;
                    foreach (var docItem in doc.Items)
                    {
                        if (docItem is BodyDocItem bdi2 && bdi2.BodyItem is BlockItem refBlock)
                        {
                            if (refBlock.Block.Kind.Name == field.RefSchema)
                            {
                                var refBlockId = refBlock.Block.InlineId switch
                                {
                                    LiteralInlineId lit => lit.Lit.Value,
                                    _ => null,
                                };
                                if (refBlockId == refId) { found = true; break; }
                            }
                        }
                    }
                    if (!found)
                        diags.ErrorWithCode("E076",
                            $"@ref target not found: no {field.RefSchema} with id '{refId}'", span);
                }

                // E077: @id_pattern validation
                if (field.IdPattern != null && (val.Kind == WclValueKind.String || val.Kind == WclValueKind.Identifier))
                {
                    var valStr = val.Kind == WclValueKind.String ? val.AsString() : val.AsIdentifier();
                    if (!Regex.IsMatch(valStr, field.IdPattern))
                        diags.ErrorWithCode("E077",
                            $"'{field.Name}' value '{valStr}' does not match @id_pattern '{field.IdPattern}'", span);
                }
            }

            // Recurse into children
            foreach (var bodyItem in block.Body)
            {
                if (bodyItem is BlockItem childBlock)
                    ValidateBlock(childBlock.Block, values, doc, diags);
            }
        }

        private static string GetStringLitValue(StringLit sl)
        {
            if (sl.Parts.Count == 1 && sl.Parts[0] is LiteralPart lp) return lp.Value;
            var sb = new System.Text.StringBuilder();
            foreach (var part in sl.Parts)
                if (part is LiteralPart lp2) sb.Append(lp2.Value);
            return sb.ToString();
        }

        public ResolvedSchema? GetSchema(string name) =>
            _schemas.TryGetValue(name, out var s) ? s : null;
    }

    public static class TypeChecker
    {
        public static bool CheckType(WclValue value, TypeExpr typeExpr)
        {
            switch (typeExpr)
            {
                case AnyTypeExpr _: return true;
                case StringTypeExpr _: return value.Kind == WclValueKind.String;
                case IntTypeExpr _: return value.Kind == WclValueKind.Int;
                case FloatTypeExpr _: return value.Kind == WclValueKind.Float;
                case BoolTypeExpr _: return value.Kind == WclValueKind.Bool;
                case NullTypeExpr _: return value.Kind == WclValueKind.Null;
                case IdentifierTypeExpr _: return value.Kind == WclValueKind.Identifier;
                case ListTypeExpr lt:
                    if (value.Kind != WclValueKind.List) return false;
                    return value.AsList().All(item => CheckType(item, lt.Inner));
                case MapTypeExpr _:
                    return value.Kind == WclValueKind.Map;
                case SetTypeExpr _:
                    return value.Kind == WclValueKind.Set;
                case UnionTypeExpr ut:
                    return ut.Types.Any(t => CheckType(value, t));
                default: return true;
            }
        }

        public static string TypeName(TypeExpr typeExpr) => typeExpr switch
        {
            StringTypeExpr _ => "string",
            IntTypeExpr _ => "int",
            FloatTypeExpr _ => "float",
            BoolTypeExpr _ => "bool",
            NullTypeExpr _ => "null",
            IdentifierTypeExpr _ => "identifier",
            AnyTypeExpr _ => "any",
            ListTypeExpr lt => $"list({TypeName(lt.Inner)})",
            MapTypeExpr mt => $"map({TypeName(mt.KeyType)}, {TypeName(mt.ValueType)})",
            SetTypeExpr st => $"set({TypeName(st.Inner)})",
            UnionTypeExpr ut => $"union({string.Join(", ", ut.Types.Select(TypeName))})",
            _ => "unknown",
        };
    }
}
