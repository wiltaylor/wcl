using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;
using Wcl.Core;
using Wcl.Core.Ast;
using Wcl.Eval.Functions;

namespace Wcl.Eval
{
    public class Evaluator
    {
        private ScopeArena _scopes = new ScopeArena();
        private DiagnosticBag _diagnostics = new DiagnosticBag();
        private readonly FunctionRegistry _builtins;
        private readonly FunctionRegistry? _customFunctions;
        private readonly HashSet<string> _declaredFunctions = new HashSet<string>();

        public Evaluator() : this(null) { }

        public Evaluator(FunctionRegistry? customFunctions)
        {
            _builtins = BuiltinRegistry.Build();
            _customFunctions = customFunctions;
        }

        public static Evaluator WithFunctions(FunctionRegistry? functions) => new Evaluator(functions);

        public ScopeArena Scopes => _scopes;
        public ScopeArena ScopesMut() => _scopes;
        public DiagnosticBag IntoDiagnostics() => _diagnostics;

        public (ScopeArena, DiagnosticBag) IntoParts()
        {
            var s = _scopes;
            var d = _diagnostics;
            _scopes = new ScopeArena();
            _diagnostics = new DiagnosticBag();
            return (s, d);
        }

        // ── Two-pass evaluation with dependency tracking ──

        public OrderedMap<string, WclValue> Evaluate(Document doc)
        {
            var moduleScope = _scopes.CreateScope(ScopeKind.Module, null);

            // Pass 1: Register all entries (unevaluated) and collect dependencies
            foreach (var item in doc.Items)
            {
                switch (item)
                {
                    case BodyDocItem bodyDoc:
                        RegisterBodyItem(bodyDoc.BodyItem, moduleScope);
                        break;
                    case ExportLetItem exportLet:
                    {
                        var deps = FindDependencies(exportLet.ExportLet.Value);
                        var entry = new ScopeEntry(exportLet.ExportLet.Name.Name,
                            ScopeEntryKind.ExportLet, null, exportLet.ExportLet.Span);
                        entry.Dependencies = deps;
                        _scopes.AddEntry(moduleScope, entry);
                        break;
                    }
                    case FunctionDeclItem funcDecl:
                        _declaredFunctions.Add(funcDecl.FunctionDecl.Name.Name);
                        break;
                }
            }

            // Topological sort for dependency ordering
            var (order, cycle) = _scopes.TopoSort(moduleScope);
            if (cycle != null)
            {
                _diagnostics.ErrorWithCode("E041",
                    $"circular dependency detected: {string.Join(" -> ", cycle)}",
                    Span.Dummy());
                // Fall back to document order
                order = _scopes.Get(moduleScope).Entries.Select(e => e.Name).ToList();
            }

            // Pass 2: Evaluate in dependency order
            var values = new OrderedMap<string, WclValue>();
            var evaluatedNames = new HashSet<string>();
            foreach (var name in order!)
            {
                if (evaluatedNames.Contains(name)) continue;
                evaluatedNames.Add(name);

                var entry = _scopes.Get(moduleScope).FindLocal(name);
                if (entry == null || entry.Evaluated) continue;

                // Find ALL matching body items for this name (handles multiple blocks of same kind)
                var bodyItems = FindAllDocBodyItems(doc, name);
                if (bodyItems.Count > 0)
                {
                    foreach (var bodyItem in bodyItems)
                        EvalRegisteredItem(bodyItem, moduleScope, values, entry);
                }
                else
                {
                    var exportLet = FindExportLet(doc, name);
                    if (exportLet != null)
                    {
                        var val = EvalExprSafe(exportLet.Value, moduleScope);
                        if (val != null)
                        {
                            entry.Value = val;
                            entry.Evaluated = true;
                            values[name] = val;
                        }
                    }
                }
            }

            // Check for unused variables (W002)
            foreach (var scopeEntry in _scopes.Get(moduleScope).Entries)
            {
                if (scopeEntry.Kind == ScopeEntryKind.LetBinding && scopeEntry.ReadCount == 0)
                {
                    _diagnostics.WarningWithCode("W002",
                        $"unused variable '{scopeEntry.Name}'", scopeEntry.Span);
                }
            }

            return values;
        }

        private void RegisterBodyItem(BodyItem item, ScopeId scope)
        {
            switch (item)
            {
                case AttributeItem attr:
                {
                    var deps = FindDependencies(attr.Attribute.Value);
                    var entry = new ScopeEntry(attr.Attribute.Name.Name,
                        ScopeEntryKind.Attribute, null, attr.Attribute.Span);
                    entry.Dependencies = deps;
                    _scopes.AddEntry(scope, entry);
                    break;
                }
                case LetBindingItem let:
                {
                    // Check for shadowing (W001)
                    var shadowedSpan = _scopes.CheckShadowing(scope, let.LetBinding.Name.Name);
                    if (shadowedSpan.HasValue)
                    {
                        _diagnostics.WarningWithCode("W001",
                            $"variable '{let.LetBinding.Name.Name}' shadows an outer binding",
                            let.LetBinding.Name.Span);
                    }

                    var deps = FindDependencies(let.LetBinding.Value);
                    var entry = new ScopeEntry(let.LetBinding.Name.Name,
                        ScopeEntryKind.LetBinding, null, let.LetBinding.Span);
                    entry.Dependencies = deps;
                    _scopes.AddEntry(scope, entry);
                    break;
                }
                case BlockItem block:
                {
                    var entry = new ScopeEntry(block.Block.Kind.Name,
                        ScopeEntryKind.BlockChild, null, block.Block.Span);
                    // Blocks depend on names used in their attributes
                    var deps = new HashSet<string>();
                    foreach (var bi in block.Block.Body)
                    {
                        if (bi is AttributeItem ai)
                            foreach (var d in FindDependencies(ai.Attribute.Value))
                                deps.Add(d);
                        else if (bi is LetBindingItem li)
                            foreach (var d in FindDependencies(li.LetBinding.Value))
                                deps.Add(d);
                    }
                    entry.Dependencies = deps;
                    _scopes.AddEntry(scope, entry);
                    break;
                }
                case TableItem ti:
                {
                    var key = "table";
                    if (ti.Table.InlineId is LiteralInlineId lid) key = lid.Lit.Value;
                    var entry = new ScopeEntry(key,
                        ScopeEntryKind.BlockChild, null, ti.Table.Span);
                    _scopes.AddEntry(scope, entry);
                    break;
                }
                // These body item types don't produce scope entries but must not be silently ignored.
                // They are handled by other pipeline phases (macros, control flow, schema collection).
                case MacroDefItem _:
                case MacroCallItem _:
                case ForLoopItem _:
                case ConditionalItem _:
                case ValidationItem _:
                case SchemaItem _:
                case DecoratorSchemaBodyItem _:
                    break;
            }
        }

        private void EvalRegisteredItem(BodyItem item, ScopeId scope,
            OrderedMap<string, WclValue> values, ScopeEntry entry)
        {
            switch (item)
            {
                case AttributeItem attr:
                {
                    var val = EvalExprSafe(attr.Attribute.Value, scope);
                    if (val != null)
                    {
                        entry.Value = val;
                        entry.Evaluated = true;
                        values[attr.Attribute.Name.Name] = val;
                    }
                    break;
                }
                case LetBindingItem let:
                {
                    var val = EvalExprSafe(let.LetBinding.Value, scope);
                    if (val != null)
                    {
                        entry.Value = val;
                        entry.Evaluated = true;
                    }
                    break;
                }
                case BlockItem block:
                {
                    var blockRef = EvalBlock(block.Block, scope);
                    var blockVal = WclValue.NewBlockRef(blockRef);
                    entry.Value = blockVal;
                    entry.Evaluated = true;

                    // Add to values
                    if (values.TryGetValue(block.Block.Kind.Name, out var existing))
                    {
                        if (existing.Kind == WclValueKind.List)
                            existing.AsList().Add(blockVal);
                        else
                            values[block.Block.Kind.Name] = WclValue.NewList(
                                new List<WclValue> { existing, blockVal });
                    }
                    else
                    {
                        values[block.Block.Kind.Name] = blockVal;
                    }
                    break;
                }
                case TableItem table:
                    EvalTable(table.Table, scope, values);
                    entry.Evaluated = true;
                    break;
            }
        }

        private static List<BodyItem> FindAllDocBodyItems(Document doc, string name)
        {
            var results = new List<BodyItem>();
            foreach (var item in doc.Items)
            {
                if (item is BodyDocItem bdi)
                {
                    switch (bdi.BodyItem)
                    {
                        case AttributeItem ai when ai.Attribute.Name.Name == name:
                            results.Add(ai); break;
                        case LetBindingItem li when li.LetBinding.Name.Name == name:
                            results.Add(li); break;
                        case BlockItem bi when bi.Block.Kind.Name == name:
                            results.Add(bi); break;
                        case TableItem ti when name == "table" || (ti.Table.InlineId is LiteralInlineId lid && lid.Lit.Value == name):
                            results.Add(ti); break;
                    }
                }
            }
            return results;
        }

        private static ExportLet? FindExportLet(Document doc, string name)
        {
            foreach (var item in doc.Items)
            {
                if (item is ExportLetItem eli && eli.ExportLet.Name.Name == name)
                    return eli.ExportLet;
            }
            return null;
        }

        // ── Dependency analysis ──

        public static HashSet<string> FindDependencies(Expr expr)
        {
            var deps = new HashSet<string>();
            CollectDeps(expr, deps);
            return deps;
        }

        private static void CollectDeps(Expr expr, HashSet<string> deps)
        {
            switch (expr)
            {
                case IdentExpr e: deps.Add(e.Ident.Name); break;
                case BinaryOpExpr e: CollectDeps(e.Left, deps); CollectDeps(e.Right, deps); break;
                case UnaryOpExpr e: CollectDeps(e.Operand, deps); break;
                case TernaryExpr e: CollectDeps(e.Condition, deps); CollectDeps(e.ThenExpr, deps); CollectDeps(e.ElseExpr, deps); break;
                case MemberAccessExpr e: CollectDeps(e.Object, deps); break;
                case IndexAccessExpr e: CollectDeps(e.Object, deps); CollectDeps(e.Index, deps); break;
                case FnCallExpr e:
                    CollectDeps(e.Callee, deps);
                    foreach (var a in e.Args)
                    {
                        if (a is PositionalCallArg pa) CollectDeps(pa.Value, deps);
                        else if (a is NamedCallArg na) CollectDeps(na.Value, deps);
                    }
                    break;
                case LambdaExpr e: CollectDeps(e.Body, deps); break;
                case ListExpr e: foreach (var i in e.Items) CollectDeps(i, deps); break;
                case MapExpr e: foreach (var (_, v) in e.Entries) CollectDeps(v, deps); break;
                case SetExpr e: foreach (var i in e.Items) CollectDeps(i, deps); break;
                case BlockExprNode e:
                    foreach (var l in e.Lets) CollectDeps(l.Value, deps);
                    CollectDeps(e.FinalExpr, deps);
                    break;
                case StringLitExpr e:
                    foreach (var p in e.StringLit.Parts)
                        if (p is InterpolationPart ip) CollectDeps(ip.Expr, deps);
                    break;
                case ParenExpr e: CollectDeps(e.Inner, deps); break;
            }
        }

        // ── Block evaluation ──

        private BlockRef EvalBlock(Block block, ScopeId parentScope)
        {
            var blockScope = _scopes.CreateScope(ScopeKind.Block, parentScope);
            var attrs = new OrderedMap<string, WclValue>();
            var children = new List<BlockRef>();

            // Add self reference
            // (deferred - self will resolve to the final block)

            foreach (var bodyItem in block.Body)
            {
                switch (bodyItem)
                {
                    case AttributeItem attr:
                    {
                        var val = EvalExprSafe(attr.Attribute.Value, blockScope);
                        if (val != null)
                        {
                            attrs[attr.Attribute.Name.Name] = val;
                            _scopes.AddEntry(blockScope, new ScopeEntry(
                                attr.Attribute.Name.Name, ScopeEntryKind.Attribute, val, attr.Attribute.Span));
                        }
                        break;
                    }
                    case LetBindingItem let:
                    {
                        var val = EvalExprSafe(let.LetBinding.Value, blockScope);
                        if (val != null)
                            _scopes.AddEntry(blockScope, new ScopeEntry(
                                let.LetBinding.Name.Name, ScopeEntryKind.LetBinding, val, let.LetBinding.Span));
                        break;
                    }
                    case BlockItem childBlock:
                        children.Add(EvalBlock(childBlock.Block, blockScope));
                        break;
                }
            }

            var id = block.InlineId != null ? ResolveInlineId(block.InlineId, parentScope) : null;
            var labels = block.Labels.Select(l => ResolveStringLit(l, parentScope)).ToList();
            var decorators = block.Decorators.Select(d => EvalDecorator(d, parentScope)).ToList();

            return new BlockRef(block.Kind.Name, id, labels, attrs, children, decorators, block.Span);
        }

        private void EvalTable(Table table, ScopeId scope, OrderedMap<string, WclValue> values)
        {
            var rows = new List<WclValue>();
            foreach (var row in table.Rows)
            {
                var map = new OrderedMap<string, WclValue>();
                for (int i = 0; i < table.Columns.Count && i < row.Cells.Count; i++)
                {
                    var val = EvalExprSafe(row.Cells[i], scope);
                    if (val != null)
                        map[table.Columns[i].Name.Name] = val;
                }
                rows.Add(WclValue.NewMap(map));
            }

            var key = "table";
            if (table.InlineId != null)
                key = ResolveInlineId(table.InlineId, scope) ?? "table";

            values[key] = WclValue.NewList(rows);
        }

        private string? ResolveInlineId(InlineId inlineId, ScopeId scope)
        {
            switch (inlineId)
            {
                case LiteralInlineId lit: return lit.Lit.Value;
                case InterpolatedInlineId interp:
                    var sb = new System.Text.StringBuilder();
                    foreach (var part in interp.Parts)
                    {
                        if (part is LiteralPart lp) sb.Append(lp.Value);
                        else if (part is InterpolationPart ip)
                        {
                            var val = EvalExprSafe(ip.Expr, scope);
                            if (val != null) sb.Append(val.ToInterpString());
                        }
                    }
                    return sb.ToString();
                default: return null;
            }
        }

        public string ResolveStringLit(StringLit sl, ScopeId scope)
        {
            var sb = new System.Text.StringBuilder();
            foreach (var part in sl.Parts)
            {
                if (part is LiteralPart lp) sb.Append(lp.Value);
                else if (part is InterpolationPart ip)
                {
                    var val = EvalExprSafe(ip.Expr, scope);
                    if (val != null) sb.Append(val.ToInterpString());
                }
            }
            return sb.ToString();
        }

        private DecoratorValue EvalDecorator(Decorator d, ScopeId scope)
        {
            var args = new OrderedMap<string, WclValue>();
            foreach (var arg in d.Args)
            {
                switch (arg)
                {
                    case NamedDecoratorArg named:
                    {
                        var val = EvalExprSafe(named.Value, scope);
                        if (val != null) args[named.Name.Name] = val;
                        break;
                    }
                    case PositionalDecoratorArg pos:
                    {
                        var val = EvalExprSafe(pos.Value, scope);
                        if (val != null) args[$"_{args.Count}"] = val;
                        break;
                    }
                }
            }
            return new DecoratorValue(d.Name.Name, args);
        }

        private WclValue? EvalExprSafe(Expr expr, ScopeId scope)
        {
            try { return EvalExpr(expr, scope); }
            catch (Exception ex)
            {
                _diagnostics.Error(ex.Message, expr.GetSpan());
                return null;
            }
        }

        // ── Expression evaluation ──

        public WclValue EvalExpr(Expr expr, ScopeId scope)
        {
            switch (expr)
            {
                case IntLitExpr e: return WclValue.NewInt(e.Value);
                case FloatLitExpr e: return WclValue.NewFloat(e.Value);
                case BoolLitExpr e: return WclValue.NewBool(e.Value);
                case NullLitExpr _: return WclValue.Null;
                case StringLitExpr e: return WclValue.NewString(ResolveStringLit(e.StringLit, scope));
                case IdentExpr e:
                {
                    var val = _scopes.Resolve(scope, e.Ident.Name);
                    if (val != null) return val;
                    if (_builtins.Functions.ContainsKey(e.Ident.Name))
                        return WclValue.NewFunction(new FunctionValue(
                            new List<string>(), new BuiltinFunctionBody(e.Ident.Name)));
                    if (_customFunctions?.Functions.ContainsKey(e.Ident.Name) == true)
                        return WclValue.NewFunction(new FunctionValue(
                            new List<string>(), new BuiltinFunctionBody(e.Ident.Name)));
                    throw new EvalException("E052", $"undefined variable: {e.Ident.Name}");
                }
                case IdentifierLitExpr e: return WclValue.NewIdentifier(e.Lit.Value);
                case ListExpr e:
                    return WclValue.NewList(e.Items.Select(i => EvalExpr(i, scope)).ToList());
                case MapExpr e:
                {
                    var map = new OrderedMap<string, WclValue>();
                    foreach (var (key, val) in e.Entries)
                    {
                        string keyStr = key switch
                        {
                            IdentMapKey ik => ik.Ident.Name,
                            StringMapKey sk => ResolveStringLit(sk.StringLit, scope),
                            _ => throw new EvalException("E040", "invalid map key"),
                        };
                        map[keyStr] = EvalExpr(val, scope);
                    }
                    return WclValue.NewMap(map);
                }
                case SetExpr e:
                {
                    var items = new List<WclValue>();
                    foreach (var item in e.Items)
                    {
                        var val = EvalExpr(item, scope);
                        if (!items.Any(x => x.Equals(val)))
                            items.Add(val);
                    }
                    return WclValue.NewSet(items);
                }
                case BinaryOpExpr e: return EvalBinaryOp(e, scope);
                case UnaryOpExpr e: return EvalUnaryOp(e, scope);
                case TernaryExpr e:
                {
                    var cond = EvalExpr(e.Condition, scope);
                    if (cond.IsTruthy() == true) return EvalExpr(e.ThenExpr, scope);
                    return EvalExpr(e.ElseExpr, scope);
                }
                case MemberAccessExpr e:
                    return EvalMemberAccess(e, scope);
                case IndexAccessExpr e:
                    return EvalIndexAccess(e, scope);
                case FnCallExpr e: return EvalFnCall(e, scope);
                case LambdaExpr e:
                {
                    var parms = e.Params.Select(p => p.Name).ToList();
                    return WclValue.NewFunction(new FunctionValue(parms,
                        new UserDefinedFunctionBody(e.Body), scope));
                }
                case BlockExprNode e:
                {
                    var blockScope = _scopes.CreateScope(ScopeKind.Block, scope);
                    foreach (var let in e.Lets)
                    {
                        var val = EvalExpr(let.Value, blockScope);
                        _scopes.AddEntry(blockScope, new ScopeEntry(
                            let.Name.Name, ScopeEntryKind.LetBinding, val, let.Span));
                    }
                    return EvalExpr(e.FinalExpr, blockScope);
                }
                case RefExpr e: return WclValue.NewIdentifier(e.Id.Value);
                case ParenExpr e: return EvalExpr(e.Inner, scope);
                default:
                    throw new EvalException("E040", $"unsupported expression type: {expr.GetType().Name}");
            }
        }

        private WclValue EvalMemberAccess(MemberAccessExpr e, ScopeId scope)
        {
            var obj = EvalExpr(e.Object, scope);
            if (obj.Kind == WclValueKind.Map)
            {
                if (obj.AsMap().TryGetValue(e.Member.Name, out var val))
                    return val;
                throw new EvalException("E054", $"map does not have key '{e.Member.Name}'");
            }
            if (obj.Kind == WclValueKind.BlockRef)
            {
                var br = obj.AsBlockRef();
                if (br.Attributes.TryGetValue(e.Member.Name, out var val))
                    return val;
                switch (e.Member.Name)
                {
                    case "id": return br.Id != null ? WclValue.NewString(br.Id) : WclValue.Null;
                    case "kind": return WclValue.NewString(br.Kind);
                    case "labels": return WclValue.NewList(br.Labels.Select(l => WclValue.NewString(l)).ToList());
                    case "decorators":
                        return WclValue.NewList(br.Decorators.Select(d =>
                        {
                            var m = new OrderedMap<string, WclValue>();
                            m["name"] = WclValue.NewString(d.Name);
                            foreach (var a in d.Args) m[a.Key] = a.Value;
                            return WclValue.NewMap(m);
                        }).ToList());
                    case "children":
                        return WclValue.NewList(br.Children.Select(c => WclValue.NewBlockRef(c)).ToList());
                }
                throw new EvalException("E054", $"block does not have attribute '{e.Member.Name}'");
            }
            // String/List .length access
            if (e.Member.Name == "length")
            {
                if (obj.Kind == WclValueKind.String) return WclValue.NewInt(obj.AsString().Length);
                if (obj.Kind == WclValueKind.List) return WclValue.NewInt(obj.AsList().Count);
            }
            throw new EvalException("E054", $"cannot access member '{e.Member.Name}' on {obj.TypeName}");
        }

        private WclValue EvalIndexAccess(IndexAccessExpr e, ScopeId scope)
        {
            var obj = EvalExpr(e.Object, scope);
            var idx = EvalExpr(e.Index, scope);
            if (obj.Kind == WclValueKind.List)
            {
                var list = obj.AsList();
                int i = (int)idx.AsInt();
                if (i < 0) i += list.Count;
                if (i < 0 || i >= list.Count)
                    throw new EvalException("E054", $"index {idx.AsInt()} out of bounds (list length {list.Count})");
                return list[i];
            }
            if (obj.Kind == WclValueKind.Map)
            {
                var key = idx.Kind == WclValueKind.String ? idx.AsString() : idx.ToInterpString();
                if (obj.AsMap().TryGetValue(key, out var val)) return val;
                throw new EvalException("E054", $"map key not found: {key}");
            }
            throw new EvalException("E054", $"cannot index {obj.TypeName}");
        }

        private WclValue EvalBinaryOp(BinaryOpExpr e, ScopeId scope)
        {
            if (e.Op == BinOp.And)
            {
                var lhs = EvalExpr(e.Left, scope);
                if (lhs.IsTruthy() == false) return WclValue.NewBool(false);
                var rhs = EvalExpr(e.Right, scope);
                return WclValue.NewBool(rhs.IsTruthy() == true);
            }
            if (e.Op == BinOp.Or)
            {
                var lhs = EvalExpr(e.Left, scope);
                if (lhs.IsTruthy() == true) return WclValue.NewBool(true);
                var rhs = EvalExpr(e.Right, scope);
                return WclValue.NewBool(rhs.IsTruthy() == true);
            }

            var left = EvalExpr(e.Left, scope);
            var right = EvalExpr(e.Right, scope);

            // String concatenation
            if (e.Op == BinOp.Add && left.Kind == WclValueKind.String)
                return WclValue.NewString(left.AsString() + right.ToInterpString());
            if (e.Op == BinOp.Add && right.Kind == WclValueKind.String)
                return WclValue.NewString(left.ToInterpString() + right.AsString());

            // List concatenation
            if (e.Op == BinOp.Add && left.Kind == WclValueKind.List && right.Kind == WclValueKind.List)
            {
                var result = new List<WclValue>(left.AsList());
                result.AddRange(right.AsList());
                return WclValue.NewList(result);
            }

            if (e.Op == BinOp.Match)
                return WclValue.NewBool(Regex.IsMatch(left.AsString(), right.AsString()));

            if (e.Op == BinOp.Eq) return WclValue.NewBool(left.Equals(right));
            if (e.Op == BinOp.Neq) return WclValue.NewBool(!left.Equals(right));

            if (IsNumeric(left) && IsNumeric(right))
            {
                if (left.Kind == WclValueKind.Int && right.Kind == WclValueKind.Int)
                {
                    long a = left.AsInt(), b = right.AsInt();
                    return e.Op switch
                    {
                        BinOp.Add => WclValue.NewInt(a + b),
                        BinOp.Sub => WclValue.NewInt(a - b),
                        BinOp.Mul => WclValue.NewInt(a * b),
                        BinOp.Div => b != 0 ? WclValue.NewInt(a / b) : throw new EvalException("E051", "division by zero"),
                        BinOp.Mod => b != 0 ? WclValue.NewInt(a % b) : throw new EvalException("E051", "modulo by zero"),
                        BinOp.Lt => WclValue.NewBool(a < b),
                        BinOp.Gt => WclValue.NewBool(a > b),
                        BinOp.Lte => WclValue.NewBool(a <= b),
                        BinOp.Gte => WclValue.NewBool(a >= b),
                        _ => throw new EvalException("E040", $"unsupported op {e.Op} for int"),
                    };
                }

                double fa = left.Kind == WclValueKind.Int ? left.AsInt() : left.AsFloat();
                double fb = right.Kind == WclValueKind.Int ? right.AsInt() : right.AsFloat();
                return e.Op switch
                {
                    BinOp.Add => WclValue.NewFloat(fa + fb),
                    BinOp.Sub => WclValue.NewFloat(fa - fb),
                    BinOp.Mul => WclValue.NewFloat(fa * fb),
                    BinOp.Div => fb != 0 ? WclValue.NewFloat(fa / fb) : throw new EvalException("E051", "division by zero"),
                    BinOp.Mod => fb != 0 ? WclValue.NewFloat(fa % fb) : throw new EvalException("E051", "modulo by zero"),
                    BinOp.Lt => WclValue.NewBool(fa < fb),
                    BinOp.Gt => WclValue.NewBool(fa > fb),
                    BinOp.Lte => WclValue.NewBool(fa <= fb),
                    BinOp.Gte => WclValue.NewBool(fa >= fb),
                    _ => throw new EvalException("E040", $"unsupported op {e.Op}"),
                };
            }

            if (left.Kind == WclValueKind.String && right.Kind == WclValueKind.String)
            {
                int cmp = string.Compare(left.AsString(), right.AsString(), StringComparison.Ordinal);
                return e.Op switch
                {
                    BinOp.Lt => WclValue.NewBool(cmp < 0),
                    BinOp.Gt => WclValue.NewBool(cmp > 0),
                    BinOp.Lte => WclValue.NewBool(cmp <= 0),
                    BinOp.Gte => WclValue.NewBool(cmp >= 0),
                    _ => throw new EvalException("E040", $"unsupported op {e.Op} for string"),
                };
            }

            throw new EvalException("E040", $"cannot apply {e.Op} to {left.TypeName} and {right.TypeName}");
        }

        private static bool IsNumeric(WclValue v) =>
            v.Kind == WclValueKind.Int || v.Kind == WclValueKind.Float;

        private WclValue EvalUnaryOp(UnaryOpExpr e, ScopeId scope)
        {
            var val = EvalExpr(e.Operand, scope);
            return e.Op switch
            {
                UnaryOp.Not => WclValue.NewBool(val.IsTruthy() != true),
                UnaryOp.Neg => val.Kind == WclValueKind.Int
                    ? WclValue.NewInt(-val.AsInt())
                    : WclValue.NewFloat(-val.AsFloat()),
                _ => throw new EvalException("E040", $"unsupported unary op {e.Op}"),
            };
        }

        private WclValue EvalFnCall(FnCallExpr e, ScopeId scope)
        {
            if (e.Callee is IdentExpr callee)
            {
                var name = callee.Ident.Name;

                if (_declaredFunctions.Contains(name) &&
                    !_builtins.Functions.ContainsKey(name) &&
                    _customFunctions?.Functions.ContainsKey(name) != true)
                {
                    _diagnostics.ErrorWithCode("E053",
                        $"function '{name}' is declared in library but not registered",
                        e.GetSpan());
                    return WclValue.Null;
                }

                switch (name)
                {
                    case "map": return EvalHigherOrder("map", e.Args, scope);
                    case "filter": return EvalHigherOrder("filter", e.Args, scope);
                    case "every": return EvalHigherOrder("every", e.Args, scope);
                    case "some": return EvalHigherOrder("some", e.Args, scope);
                    case "reduce": return EvalReduce(e.Args, scope);
                    case "count": return EvalHigherOrder("count", e.Args, scope);
                }
            }

            var calleeVal = EvalExpr(e.Callee, scope);
            var args = e.Args.Select(a => a switch
            {
                PositionalCallArg pa => EvalExpr(pa.Value, scope),
                NamedCallArg na => EvalExpr(na.Value, scope),
                _ => WclValue.Null,
            }).ToArray();

            if (calleeVal.Kind == WclValueKind.Function)
            {
                var fn = calleeVal.AsFunction();
                if (fn.Body is BuiltinFunctionBody builtin)
                {
                    if (_customFunctions?.Functions.TryGetValue(builtin.Name, out var customFn) == true)
                        return customFn(args);
                    if (_builtins.Functions.TryGetValue(builtin.Name, out var builtinFn))
                        return builtinFn(args);
                    throw new EvalException("E052", $"unknown builtin: {builtin.Name}");
                }
                if (fn.Body is UserDefinedFunctionBody userDef)
                {
                    var lambdaScope = _scopes.CreateScope(ScopeKind.Lambda,
                        fn.ClosureScope ?? scope);
                    for (int i = 0; i < fn.Params.Count && i < args.Length; i++)
                        _scopes.AddEntry(lambdaScope, new ScopeEntry(
                            fn.Params[i], ScopeEntryKind.Parameter, args[i], Span.Dummy()));
                    return EvalExpr(userDef.Expr, lambdaScope);
                }
            }

            if (e.Callee is IdentExpr ident)
            {
                var name = ident.Ident.Name;
                if (_customFunctions?.Functions.TryGetValue(name, out var cfn) == true)
                    return cfn(args);
                if (_builtins.Functions.TryGetValue(name, out var bfn))
                    return bfn(args);
            }

            throw new EvalException("E040", $"not callable: {calleeVal.TypeName}");
        }

        private WclValue EvalHigherOrder(string name, List<CallArg> args, ScopeId scope)
        {
            if (args.Count < 2)
                throw new EvalException("E040", $"{name} requires at least 2 arguments");
            var listVal = EvalExpr(((PositionalCallArg)args[0]).Value, scope);
            if (listVal.Kind != WclValueKind.List)
                throw new EvalException("E040", $"{name} first argument must be a list, got {listVal.TypeName}");
            var list = listVal.AsList();
            var lambdaExpr = ((PositionalCallArg)args[1]).Value;

            switch (name)
            {
                case "map":
                    return WclValue.NewList(list.Select(item => ApplyLambda(lambdaExpr, scope, item)).ToList());
                case "filter":
                    return WclValue.NewList(list.Where(item => ApplyLambda(lambdaExpr, scope, item).IsTruthy() == true).ToList());
                case "every":
                    return WclValue.NewBool(list.All(item => ApplyLambda(lambdaExpr, scope, item).IsTruthy() == true));
                case "some":
                    return WclValue.NewBool(list.Any(item => ApplyLambda(lambdaExpr, scope, item).IsTruthy() == true));
                case "count":
                    return WclValue.NewInt(list.Count(item => ApplyLambda(lambdaExpr, scope, item).IsTruthy() == true));
                default:
                    throw new EvalException("E040", $"unknown higher-order: {name}");
            }
        }

        private WclValue EvalReduce(List<CallArg> args, ScopeId scope)
        {
            if (args.Count < 3)
                throw new EvalException("E040", "reduce requires 3 arguments");
            var list = EvalExpr(((PositionalCallArg)args[0]).Value, scope).AsList();
            var acc = EvalExpr(((PositionalCallArg)args[1]).Value, scope);
            var lambdaExpr = ((PositionalCallArg)args[2]).Value;

            foreach (var item in list)
                acc = ApplyLambda2(lambdaExpr, scope, acc, item);
            return acc;
        }

        private WclValue ApplyLambda(Expr lambdaExpr, ScopeId scope, WclValue arg)
        {
            var fn = EvalExpr(lambdaExpr, scope);
            if (fn.Kind != WclValueKind.Function)
                throw new EvalException("E040", "expected function argument");
            var fv = fn.AsFunction();
            var lambdaScope = _scopes.CreateScope(ScopeKind.Lambda, fv.ClosureScope ?? scope);
            if (fv.Params.Count > 0)
                _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[0], ScopeEntryKind.Parameter, arg, Span.Dummy()));
            if (fv.Body is UserDefinedFunctionBody ud)
                return EvalExpr(ud.Expr, lambdaScope);
            if (fv.Body is BuiltinFunctionBody bi && _builtins.Functions.TryGetValue(bi.Name, out var bfn))
                return bfn(new[] { arg });
            throw new EvalException("E040", "cannot apply function");
        }

        private WclValue ApplyLambda2(Expr lambdaExpr, ScopeId scope, WclValue arg1, WclValue arg2)
        {
            var fn = EvalExpr(lambdaExpr, scope);
            if (fn.Kind != WclValueKind.Function)
                throw new EvalException("E040", "expected function argument");
            var fv = fn.AsFunction();
            var lambdaScope = _scopes.CreateScope(ScopeKind.Lambda, fv.ClosureScope ?? scope);
            if (fv.Params.Count > 0)
                _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[0], ScopeEntryKind.Parameter, arg1, Span.Dummy()));
            if (fv.Params.Count > 1)
                _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[1], ScopeEntryKind.Parameter, arg2, Span.Dummy()));
            if (fv.Body is UserDefinedFunctionBody ud)
                return EvalExpr(ud.Expr, lambdaScope);
            throw new EvalException("E040", "cannot apply function");
        }
    }

    public class EvalException : Exception
    {
        public string Code { get; }
        public EvalException(string code, string message) : base(message) { Code = code; }
    }
}
