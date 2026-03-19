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

        public static Evaluator WithFunctions(FunctionRegistry? functions)
        {
            return new Evaluator(functions);
        }

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

        public OrderedMap<string, WclValue> Evaluate(Document doc)
        {
            var moduleScope = _scopes.CreateScope(ScopeKind.Module, null);
            var values = new OrderedMap<string, WclValue>();

            foreach (var item in doc.Items)
            {
                switch (item)
                {
                    case BodyDocItem bodyDoc:
                        EvalBodyItem(bodyDoc.BodyItem, moduleScope, values);
                        break;
                    case ExportLetItem exportLet:
                    {
                        var val = EvalExprSafe(exportLet.ExportLet.Value, moduleScope);
                        if (val != null)
                        {
                            values[exportLet.ExportLet.Name.Name] = val;
                            _scopes.AddEntry(moduleScope, new ScopeEntry(
                                exportLet.ExportLet.Name.Name, ScopeEntryKind.LetBinding, val, exportLet.ExportLet.Span));
                        }
                        break;
                    }
                    case FunctionDeclItem funcDecl:
                        _declaredFunctions.Add(funcDecl.FunctionDecl.Name.Name);
                        break;
                }
            }

            return values;
        }

        private void EvalBodyItem(BodyItem item, ScopeId scope, OrderedMap<string, WclValue> values)
        {
            switch (item)
            {
                case AttributeItem attr:
                {
                    var val = EvalExprSafe(attr.Attribute.Value, scope);
                    if (val != null)
                    {
                        values[attr.Attribute.Name.Name] = val;
                        _scopes.AddEntry(scope, new ScopeEntry(
                            attr.Attribute.Name.Name, ScopeEntryKind.Attribute, val, attr.Attribute.Span));
                    }
                    break;
                }
                case LetBindingItem let:
                {
                    var val = EvalExprSafe(let.LetBinding.Value, scope);
                    if (val != null)
                    {
                        _scopes.AddEntry(scope, new ScopeEntry(
                            let.LetBinding.Name.Name, ScopeEntryKind.LetBinding, val, let.LetBinding.Span));
                    }
                    break;
                }
                case BlockItem block:
                {
                    var blockRef = EvalBlock(block.Block, scope);
                    var blockVal = WclValue.NewBlockRef(blockRef);

                    // Store as kind or kind#id
                    var key = block.Block.Kind.Name;
                    if (block.Block.InlineId != null)
                    {
                        var id = ResolveInlineId(block.Block.InlineId, scope);
                        key = $"{key}#{id}";
                    }

                    // Add to values - if multiple blocks of same kind, collect in a list
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
                    break;
            }
        }

        private BlockRef EvalBlock(Block block, ScopeId parentScope)
        {
            var blockScope = _scopes.CreateScope(ScopeKind.Block, parentScope);
            var attrs = new OrderedMap<string, WclValue>();
            var children = new List<BlockRef>();

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
            // Tables become lists of maps
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

        private string ResolveStringLit(StringLit sl, ScopeId scope)
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
                    // Check if it's a builtin function name
                    if (_builtins.Functions.ContainsKey(e.Ident.Name))
                        return WclValue.NewFunction(new FunctionValue(
                            new List<string>(), new BuiltinFunctionBody(e.Ident.Name)));
                    if (_customFunctions?.Functions.ContainsKey(e.Ident.Name) == true)
                        return WclValue.NewFunction(new FunctionValue(
                            new List<string>(), new BuiltinFunctionBody(e.Ident.Name)));
                    throw new Exception($"undefined variable: {e.Ident.Name}");
                }
                case IdentifierLitExpr e: return WclValue.NewIdentifier(e.Lit.Value);
                case ListExpr e:
                {
                    var items = new List<WclValue>();
                    foreach (var item in e.Items)
                        items.Add(EvalExpr(item, scope));
                    return WclValue.NewList(items);
                }
                case MapExpr e:
                {
                    var map = new OrderedMap<string, WclValue>();
                    foreach (var (key, val) in e.Entries)
                    {
                        string keyStr = key switch
                        {
                            IdentMapKey ik => ik.Ident.Name,
                            StringMapKey sk => ResolveStringLit(sk.StringLit, scope),
                            _ => throw new Exception("invalid map key"),
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
                {
                    var obj = EvalExpr(e.Object, scope);
                    if (obj.Kind == WclValueKind.Map)
                    {
                        if (obj.AsMap().TryGetValue(e.Member.Name, out var val))
                            return val;
                        throw new Exception($"map does not have key '{e.Member.Name}'");
                    }
                    if (obj.Kind == WclValueKind.BlockRef)
                    {
                        var br = obj.AsBlockRef();
                        if (br.Attributes.TryGetValue(e.Member.Name, out var val))
                            return val;
                        // Check for id, kind, labels, decorators
                        if (e.Member.Name == "id") return br.Id != null ? WclValue.NewString(br.Id) : WclValue.Null;
                        if (e.Member.Name == "kind") return WclValue.NewString(br.Kind);
                        throw new Exception($"block does not have attribute '{e.Member.Name}'");
                    }
                    throw new Exception($"cannot access member on {obj.TypeName}");
                }
                case IndexAccessExpr e:
                {
                    var obj = EvalExpr(e.Object, scope);
                    var idx = EvalExpr(e.Index, scope);
                    if (obj.Kind == WclValueKind.List)
                    {
                        var list = obj.AsList();
                        int i = (int)idx.AsInt();
                        if (i < 0) i += list.Count;
                        return list[i];
                    }
                    if (obj.Kind == WclValueKind.Map)
                    {
                        var key = idx.Kind == WclValueKind.String ? idx.AsString() : idx.ToInterpString();
                        if (obj.AsMap().TryGetValue(key, out var val)) return val;
                        throw new Exception($"map key not found: {key}");
                    }
                    throw new Exception($"cannot index {obj.TypeName}");
                }
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
                    throw new Exception($"unsupported expression type: {expr.GetType().Name}");
            }
        }

        private WclValue EvalBinaryOp(BinaryOpExpr e, ScopeId scope)
        {
            // Short-circuit for && and ||
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

            // Regex match
            if (e.Op == BinOp.Match)
                return WclValue.NewBool(Regex.IsMatch(left.AsString(), right.AsString()));

            // Equality
            if (e.Op == BinOp.Eq) return WclValue.NewBool(left.Equals(right));
            if (e.Op == BinOp.Neq) return WclValue.NewBool(!left.Equals(right));

            // Arithmetic with Int/Float promotion
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
                        BinOp.Div => b != 0 ? WclValue.NewInt(a / b) : throw new Exception("division by zero"),
                        BinOp.Mod => WclValue.NewInt(a % b),
                        BinOp.Lt => WclValue.NewBool(a < b),
                        BinOp.Gt => WclValue.NewBool(a > b),
                        BinOp.Lte => WclValue.NewBool(a <= b),
                        BinOp.Gte => WclValue.NewBool(a >= b),
                        _ => throw new Exception($"unsupported op {e.Op} for int"),
                    };
                }

                double fa = left.Kind == WclValueKind.Int ? left.AsInt() : left.AsFloat();
                double fb = right.Kind == WclValueKind.Int ? right.AsInt() : right.AsFloat();
                return e.Op switch
                {
                    BinOp.Add => WclValue.NewFloat(fa + fb),
                    BinOp.Sub => WclValue.NewFloat(fa - fb),
                    BinOp.Mul => WclValue.NewFloat(fa * fb),
                    BinOp.Div => fb != 0 ? WclValue.NewFloat(fa / fb) : throw new Exception("division by zero"),
                    BinOp.Mod => WclValue.NewFloat(fa % fb),
                    BinOp.Lt => WclValue.NewBool(fa < fb),
                    BinOp.Gt => WclValue.NewBool(fa > fb),
                    BinOp.Lte => WclValue.NewBool(fa <= fb),
                    BinOp.Gte => WclValue.NewBool(fa >= fb),
                    _ => throw new Exception($"unsupported op {e.Op}"),
                };
            }

            // String comparison
            if (left.Kind == WclValueKind.String && right.Kind == WclValueKind.String)
            {
                int cmp = string.Compare(left.AsString(), right.AsString(), StringComparison.Ordinal);
                return e.Op switch
                {
                    BinOp.Lt => WclValue.NewBool(cmp < 0),
                    BinOp.Gt => WclValue.NewBool(cmp > 0),
                    BinOp.Lte => WclValue.NewBool(cmp <= 0),
                    BinOp.Gte => WclValue.NewBool(cmp >= 0),
                    _ => throw new Exception($"unsupported op {e.Op} for string"),
                };
            }

            throw new Exception($"cannot apply {e.Op} to {left.TypeName} and {right.TypeName}");
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
                _ => throw new Exception($"unsupported unary op {e.Op}"),
            };
        }

        private WclValue EvalFnCall(FnCallExpr e, ScopeId scope)
        {
            // Special higher-order functions
            if (e.Callee is IdentExpr callee)
            {
                var name = callee.Ident.Name;

                // Check for declared but unregistered functions
                if (_declaredFunctions.Contains(name) &&
                    !_builtins.Functions.ContainsKey(name) &&
                    _customFunctions?.Functions.ContainsKey(name) != true)
                {
                    _diagnostics.ErrorWithCode("E053",
                        $"function '{name}' is declared in library but not registered",
                        e.GetSpan());
                    return WclValue.Null;
                }

                // Higher-order functions with lazy lambda evaluation
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

            // Evaluate callee
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
                    // Try custom first, then builtins
                    if (_customFunctions?.Functions.TryGetValue(builtin.Name, out var customFn) == true)
                        return customFn(args);
                    if (_builtins.Functions.TryGetValue(builtin.Name, out var builtinFn))
                        return builtinFn(args);
                    throw new Exception($"unknown builtin: {builtin.Name}");
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

            // Maybe it's a direct builtin call
            if (e.Callee is IdentExpr ident)
            {
                var name = ident.Ident.Name;
                if (_customFunctions?.Functions.TryGetValue(name, out var cfn) == true)
                    return cfn(args);
                if (_builtins.Functions.TryGetValue(name, out var bfn))
                    return bfn(args);
            }

            throw new Exception($"not callable: {calleeVal.TypeName}");
        }

        private WclValue EvalHigherOrder(string name, List<CallArg> args, ScopeId scope)
        {
            if (args.Count < 2) throw new Exception($"{name} requires at least 2 arguments");
            var listVal = EvalExpr(((PositionalCallArg)args[0]).Value, scope);
            var list = listVal.AsList();
            var lambdaExpr = ((PositionalCallArg)args[1]).Value;

            switch (name)
            {
                case "map":
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                        result.Add(ApplyLambda(lambdaExpr, scope, item));
                    return WclValue.NewList(result);
                }
                case "filter":
                {
                    var result = new List<WclValue>();
                    foreach (var item in list)
                    {
                        var pred = ApplyLambda(lambdaExpr, scope, item);
                        if (pred.IsTruthy() == true) result.Add(item);
                    }
                    return WclValue.NewList(result);
                }
                case "every":
                {
                    foreach (var item in list)
                    {
                        var pred = ApplyLambda(lambdaExpr, scope, item);
                        if (pred.IsTruthy() != true) return WclValue.NewBool(false);
                    }
                    return WclValue.NewBool(true);
                }
                case "some":
                {
                    foreach (var item in list)
                    {
                        var pred = ApplyLambda(lambdaExpr, scope, item);
                        if (pred.IsTruthy() == true) return WclValue.NewBool(true);
                    }
                    return WclValue.NewBool(false);
                }
                case "count":
                {
                    int count = 0;
                    foreach (var item in list)
                    {
                        var pred = ApplyLambda(lambdaExpr, scope, item);
                        if (pred.IsTruthy() == true) count++;
                    }
                    return WclValue.NewInt(count);
                }
                default: throw new Exception($"unknown higher-order: {name}");
            }
        }

        private WclValue EvalReduce(List<CallArg> args, ScopeId scope)
        {
            var list = EvalExpr(((PositionalCallArg)args[0]).Value, scope).AsList();
            var init = EvalExpr(((PositionalCallArg)args[1]).Value, scope);
            var lambdaExpr = ((PositionalCallArg)args[2]).Value;

            var acc = init;
            foreach (var item in list)
                acc = ApplyLambda2(lambdaExpr, scope, acc, item);
            return acc;
        }

        private WclValue ApplyLambda(Expr lambdaExpr, ScopeId scope, WclValue arg)
        {
            var fn = EvalExpr(lambdaExpr, scope);
            if (fn.Kind == WclValueKind.Function)
            {
                var fv = fn.AsFunction();
                var lambdaScope = _scopes.CreateScope(ScopeKind.Lambda, fv.ClosureScope ?? scope);
                if (fv.Params.Count > 0)
                    _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[0], ScopeEntryKind.Parameter, arg, Span.Dummy()));
                if (fv.Body is UserDefinedFunctionBody ud)
                    return EvalExpr(ud.Expr, lambdaScope);
                if (fv.Body is BuiltinFunctionBody bi)
                {
                    if (_builtins.Functions.TryGetValue(bi.Name, out var bfn))
                        return bfn(new[] { arg });
                }
            }
            throw new Exception("expected lambda");
        }

        private WclValue ApplyLambda2(Expr lambdaExpr, ScopeId scope, WclValue arg1, WclValue arg2)
        {
            var fn = EvalExpr(lambdaExpr, scope);
            if (fn.Kind == WclValueKind.Function)
            {
                var fv = fn.AsFunction();
                var lambdaScope = _scopes.CreateScope(ScopeKind.Lambda, fv.ClosureScope ?? scope);
                if (fv.Params.Count > 0)
                    _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[0], ScopeEntryKind.Parameter, arg1, Span.Dummy()));
                if (fv.Params.Count > 1)
                    _scopes.AddEntry(lambdaScope, new ScopeEntry(fv.Params[1], ScopeEntryKind.Parameter, arg2, Span.Dummy()));
                if (fv.Body is UserDefinedFunctionBody ud)
                    return EvalExpr(ud.Expr, lambdaScope);
            }
            throw new Exception("expected lambda");
        }
    }
}
