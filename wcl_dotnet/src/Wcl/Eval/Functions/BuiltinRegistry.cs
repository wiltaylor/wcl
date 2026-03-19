using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using Wcl.Core;

namespace Wcl.Eval.Functions
{
    public static class BuiltinRegistry
    {
        public static FunctionRegistry Build()
        {
            var reg = new FunctionRegistry();

            // String functions
            reg.Register("upper", args => WclValue.NewString(args[0].AsString().ToUpperInvariant()));
            reg.Register("lower", args => WclValue.NewString(args[0].AsString().ToLowerInvariant()));
            reg.Register("trim", args => WclValue.NewString(args[0].AsString().Trim()));
            reg.Register("trim_prefix", args => {
                var s = args[0].AsString(); var p = args[1].AsString();
                return WclValue.NewString(s.StartsWith(p) ? s.Substring(p.Length) : s);
            });
            reg.Register("trim_suffix", args => {
                var s = args[0].AsString(); var p = args[1].AsString();
                return WclValue.NewString(s.EndsWith(p) ? s.Substring(0, s.Length - p.Length) : s);
            });
            reg.Register("replace", args => WclValue.NewString(args[0].AsString().Replace(args[1].AsString(), args[2].AsString())));
            reg.Register("split", args => {
                var sep = args[0].AsString();   // arg 0 = separator
                var s = args[1].AsString();     // arg 1 = string to split
                var parts = s.Split(new[] { sep }, StringSplitOptions.None);
                return WclValue.NewList(parts.Select(p => WclValue.NewString(p)).ToList());
            });
            reg.Register("join", args => {
                var list = args[0].AsList();
                var sep = args[1].AsString();
                return WclValue.NewString(string.Join(sep, list.Select(v => v.ToInterpString())));
            });
            reg.Register("starts_with", args => WclValue.NewBool(args[0].AsString().StartsWith(args[1].AsString())));
            reg.Register("ends_with", args => WclValue.NewBool(args[0].AsString().EndsWith(args[1].AsString())));
            reg.Register("contains", args => {
                if (args[0].Kind == WclValueKind.List)
                    return WclValue.NewBool(args[0].AsList().Any(item => item.Equals(args[1])));
                return WclValue.NewBool(args[0].AsString().Contains(args[1].AsString()));
            });
            reg.Register("length", args => WclValue.NewInt(args[0].AsString().Length));
            reg.Register("substr", args => {
                var s = args[0].AsString();
                int len = s.Length;
                int start = (int)Math.Max(0, Math.Min(args[1].AsInt(), len));
                int end = args.Length > 2
                    ? (int)Math.Max(0, Math.Min(args[2].AsInt(), len))
                    : len;
                end = Math.Max(end, start);
                return WclValue.NewString(s.Substring(start, end - start));
            });
            reg.Register("format", args => {
                var fmt = args[0].AsString();
                var sb = new StringBuilder();
                int argIdx = 0;
                for (int i = 0; i < fmt.Length; i++)
                {
                    if (fmt[i] == '{' && i + 1 < fmt.Length && fmt[i + 1] == '}')
                    {
                        if (argIdx + 1 < args.Length)
                            sb.Append(args[argIdx + 1].ToInterpString());
                        else
                            throw new Exception($"format: not enough arguments (placeholder {argIdx} but only {args.Length - 1} args)");
                        argIdx++;
                        i++; // skip }
                    }
                    else
                    {
                        sb.Append(fmt[i]);
                    }
                }
                return WclValue.NewString(sb.ToString());
            });
            reg.Register("regex_match", args => {
                var input = args[0].AsString();
                var pattern = args[1].AsString();
                return WclValue.NewBool(Regex.IsMatch(input, pattern));
            });
            reg.Register("regex_capture", args => {
                var input = args[0].AsString();
                var pattern = args[1].AsString();
                var match = Regex.Match(input, pattern);
                if (!match.Success) return WclValue.NewList(new List<WclValue>());
                var captures = new List<WclValue>();
                for (int i = 1; i < match.Groups.Count; i++)
                    captures.Add(WclValue.NewString(match.Groups[i].Value));
                return WclValue.NewList(captures);
            });

            // Math functions
            reg.Register("abs", args => args[0].Kind == WclValueKind.Int
                ? WclValue.NewInt(Math.Abs(args[0].AsInt()))
                : WclValue.NewFloat(Math.Abs(args[0].AsFloat())));
            reg.Register("min", args => {
                if (args[0].Kind == WclValueKind.Int && args[1].Kind == WclValueKind.Int)
                    return WclValue.NewInt(Math.Min(args[0].AsInt(), args[1].AsInt()));
                double a = args[0].Kind == WclValueKind.Int ? args[0].AsInt() : args[0].AsFloat();
                double b = args[1].Kind == WclValueKind.Int ? args[1].AsInt() : args[1].AsFloat();
                return WclValue.NewFloat(Math.Min(a, b));
            });
            reg.Register("max", args => {
                if (args[0].Kind == WclValueKind.Int && args[1].Kind == WclValueKind.Int)
                    return WclValue.NewInt(Math.Max(args[0].AsInt(), args[1].AsInt()));
                double a = args[0].Kind == WclValueKind.Int ? args[0].AsInt() : args[0].AsFloat();
                double b = args[1].Kind == WclValueKind.Int ? args[1].AsInt() : args[1].AsFloat();
                return WclValue.NewFloat(Math.Max(a, b));
            });
            reg.Register("floor", args => WclValue.NewInt((long)Math.Floor(args[0].AsFloat())));
            reg.Register("ceil", args => WclValue.NewInt((long)Math.Ceiling(args[0].AsFloat())));
            reg.Register("round", args => WclValue.NewInt((long)Math.Round(args[0].AsFloat())));
            reg.Register("sqrt", args => WclValue.NewFloat(Math.Sqrt(
                args[0].Kind == WclValueKind.Int ? args[0].AsInt() : args[0].AsFloat())));
            reg.Register("pow", args => {
                double b = args[0].Kind == WclValueKind.Int ? args[0].AsInt() : args[0].AsFloat();
                double e = args[1].Kind == WclValueKind.Int ? args[1].AsInt() : args[1].AsFloat();
                return WclValue.NewFloat(Math.Pow(b, e));
            });

            // Collection functions
            reg.Register("len", args => {
                switch (args[0].Kind)
                {
                    case WclValueKind.String: return WclValue.NewInt(args[0].AsString().Length);
                    case WclValueKind.List: return WclValue.NewInt(args[0].AsList().Count);
                    case WclValueKind.Map: return WclValue.NewInt(args[0].AsMap().Count);
                    case WclValueKind.Set: return WclValue.NewInt(args[0].AsSet().Count);
                    default: throw new Exception($"len: unsupported type {args[0].TypeName}");
                }
            });
            reg.Register("keys", args => {
                var map = args[0].AsMap();
                return WclValue.NewList(map.Keys.Select(k => WclValue.NewString(k)).ToList());
            });
            reg.Register("values", args => {
                var map = args[0].AsMap();
                return WclValue.NewList(map.Values.ToList());
            });
            reg.Register("flatten", args => {
                var result = new List<WclValue>();
                foreach (var item in args[0].AsList())
                {
                    if (item.Kind == WclValueKind.List)
                        result.AddRange(item.AsList());
                    else
                        result.Add(item);
                }
                return WclValue.NewList(result);
            });
            reg.Register("concat", args => {
                var result = new List<WclValue>();
                foreach (var arg in args)
                    result.AddRange(arg.AsList());
                return WclValue.NewList(result);
            });
            reg.Register("distinct", args => {
                var seen = new List<WclValue>();
                foreach (var item in args[0].AsList())
                {
                    if (!seen.Any(s => s.Equals(item)))
                        seen.Add(item);
                }
                return WclValue.NewList(seen);
            });
            reg.Register("sort", args => {
                var list = new List<WclValue>(args[0].AsList());
                list.Sort((a, b) => {
                    if (a.Kind == WclValueKind.Int && b.Kind == WclValueKind.Int)
                        return a.AsInt().CompareTo(b.AsInt());
                    if (a.Kind == WclValueKind.String && b.Kind == WclValueKind.String)
                        return string.Compare(a.AsString(), b.AsString(), StringComparison.Ordinal);
                    return string.Compare(a.ToString(), b.ToString(), StringComparison.Ordinal);
                });
                return WclValue.NewList(list);
            });
            reg.Register("reverse", args => {
                var list = new List<WclValue>(args[0].AsList());
                list.Reverse();
                return WclValue.NewList(list);
            });
            reg.Register("index_of", args => {
                var list = args[0].AsList();
                for (int i = 0; i < list.Count; i++)
                    if (list[i].Equals(args[1])) return WclValue.NewInt(i);
                return WclValue.NewInt(-1);
            });
            reg.Register("range", args => {
                var start = args[0].AsInt();
                var end = args[1].AsInt();
                long step = args.Length > 2 ? args[2].AsInt() : 1;
                if (step == 0) throw new Exception("range: step must not be zero");
                var list = new List<WclValue>();
                if (step > 0)
                {
                    for (long i = start; i < end; i += step)
                        list.Add(WclValue.NewInt(i));
                }
                else
                {
                    for (long i = start; i > end; i += step)
                        list.Add(WclValue.NewInt(i));
                }
                return WclValue.NewList(list);
            });
            reg.Register("zip", args => {
                var a = args[0].AsList();
                var b = args[1].AsList();
                var result = new List<WclValue>();
                int len = Math.Min(a.Count, b.Count);
                for (int i = 0; i < len; i++)
                    result.Add(WclValue.NewList(new List<WclValue> { a[i], b[i] }));
                return WclValue.NewList(result);
            });

            // Aggregate functions
            reg.Register("sum", args => {
                long intSum = 0; bool hasFloat = false; double floatSum = 0;
                foreach (var item in args[0].AsList())
                {
                    if (item.Kind == WclValueKind.Float) { hasFloat = true; floatSum += item.AsFloat(); }
                    else { intSum += item.AsInt(); floatSum += item.AsInt(); }
                }
                return hasFloat ? WclValue.NewFloat(floatSum) : WclValue.NewInt(intSum);
            });
            reg.Register("avg", args => {
                var list = args[0].AsList();
                if (list.Count == 0) return WclValue.NewFloat(0);
                double total = 0;
                foreach (var item in list)
                    total += item.Kind == WclValueKind.Int ? item.AsInt() : item.AsFloat();
                return WclValue.NewFloat(total / list.Count);
            });
            reg.Register("min_of", args => {
                var list = args[0].AsList();
                if (list.Count == 0) throw new Exception("min_of: empty list");
                var result = list[0];
                for (int i = 1; i < list.Count; i++)
                {
                    double a = result.Kind == WclValueKind.Int ? result.AsInt() : result.AsFloat();
                    double b = list[i].Kind == WclValueKind.Int ? list[i].AsInt() : list[i].AsFloat();
                    if (b < a) result = list[i];
                }
                return result;
            });
            reg.Register("max_of", args => {
                var list = args[0].AsList();
                if (list.Count == 0) throw new Exception("max_of: empty list");
                var result = list[0];
                for (int i = 1; i < list.Count; i++)
                {
                    double a = result.Kind == WclValueKind.Int ? result.AsInt() : result.AsFloat();
                    double b = list[i].Kind == WclValueKind.Int ? list[i].AsInt() : list[i].AsFloat();
                    if (b > a) result = list[i];
                }
                return result;
            });

            // Crypto/encoding functions
            reg.Register("sha256", args => {
                using var sha = SHA256.Create();
                var hash = sha.ComputeHash(Encoding.UTF8.GetBytes(args[0].AsString()));
                return WclValue.NewString(BitConverter.ToString(hash).Replace("-", "").ToLowerInvariant());
            });
            reg.Register("base64_encode", args => WclValue.NewString(Convert.ToBase64String(Encoding.UTF8.GetBytes(args[0].AsString()))));
            reg.Register("base64_decode", args => WclValue.NewString(Encoding.UTF8.GetString(Convert.FromBase64String(args[0].AsString()))));
            reg.Register("json_encode", args => WclValue.NewString(JsonEncode(args[0])));

            // Type functions
            reg.Register("to_string", args => WclValue.NewString(args[0].ToInterpString()));
            reg.Register("to_int", args => {
                switch (args[0].Kind)
                {
                    case WclValueKind.Int: return args[0];
                    case WclValueKind.Float: return WclValue.NewInt((long)args[0].AsFloat());
                    case WclValueKind.String:
                        if (long.TryParse(args[0].AsString(), out var i)) return WclValue.NewInt(i);
                        throw new Exception($"cannot convert '{args[0].AsString()}' to int");
                    case WclValueKind.Bool: return WclValue.NewInt(args[0].AsBool() ? 1 : 0);
                    default: throw new Exception($"cannot convert {args[0].TypeName} to int");
                }
            });
            reg.Register("to_float", args => {
                switch (args[0].Kind)
                {
                    case WclValueKind.Float: return args[0];
                    case WclValueKind.Int: return WclValue.NewFloat(args[0].AsInt());
                    case WclValueKind.String:
                        if (double.TryParse(args[0].AsString(), NumberStyles.Float, CultureInfo.InvariantCulture, out var f))
                            return WclValue.NewFloat(f);
                        throw new Exception($"cannot convert '{args[0].AsString()}' to float");
                    default: throw new Exception($"cannot convert {args[0].TypeName} to float");
                }
            });
            reg.Register("to_bool", args => {
                switch (args[0].Kind)
                {
                    case WclValueKind.Bool: return args[0];
                    case WclValueKind.String:
                        var s = args[0].AsString();
                        if (s == "true") return WclValue.NewBool(true);
                        if (s == "false") return WclValue.NewBool(false);
                        throw new Exception($"cannot convert '{s}' to bool");
                    case WclValueKind.Int: return WclValue.NewBool(args[0].AsInt() != 0);
                    default: throw new Exception($"cannot convert {args[0].TypeName} to bool");
                }
            });
            reg.Register("type_of", args => WclValue.NewString(args[0].TypeName));
            reg.Register("has", args => {
                if (args[0].Kind == WclValueKind.Map)
                    return WclValue.NewBool(args[0].AsMap().ContainsKey(args[1].AsString()));
                return WclValue.NewBool(false);
            });
            reg.Register("has_decorator", args => {
                if (args[0].Kind == WclValueKind.BlockRef)
                    return WclValue.NewBool(args[0].AsBlockRef().HasDecorator(args[1].AsString()));
                return WclValue.NewBool(false);
            });

            return reg;
        }

        private static string JsonEncode(WclValue value)
        {
            switch (value.Kind)
            {
                case WclValueKind.String:
                    return System.Text.Json.JsonSerializer.Serialize(value.AsString());
                case WclValueKind.Int:
                    return value.AsInt().ToString();
                case WclValueKind.Float:
                    return value.AsFloat().ToString(CultureInfo.InvariantCulture);
                case WclValueKind.Bool:
                    return value.AsBool() ? "true" : "false";
                case WclValueKind.Null:
                    return "null";
                case WclValueKind.List:
                {
                    var sb = new StringBuilder("[");
                    var list = value.AsList();
                    for (int i = 0; i < list.Count; i++)
                    {
                        if (i > 0) sb.Append(',');
                        sb.Append(JsonEncode(list[i]));
                    }
                    sb.Append(']');
                    return sb.ToString();
                }
                case WclValueKind.Map:
                {
                    var sb = new StringBuilder("{");
                    int idx = 0;
                    foreach (var kvp in value.AsMap())
                    {
                        if (idx++ > 0) sb.Append(',');
                        sb.Append(System.Text.Json.JsonSerializer.Serialize(kvp.Key));
                        sb.Append(':');
                        sb.Append(JsonEncode(kvp.Value));
                    }
                    sb.Append('}');
                    return sb.ToString();
                }
                default:
                    return System.Text.Json.JsonSerializer.Serialize(value.ToString());
            }
        }

        public static List<FunctionSignature> BuiltinSignatures()
        {
            return new List<FunctionSignature>
            {
                new FunctionSignature("upper", new List<string>{"s: string"}, "string", "Convert to uppercase"),
                new FunctionSignature("lower", new List<string>{"s: string"}, "string", "Convert to lowercase"),
                new FunctionSignature("trim", new List<string>{"s: string"}, "string", "Trim whitespace"),
                new FunctionSignature("trim_prefix", new List<string>{"s: string", "prefix: string"}, "string", "Trim prefix"),
                new FunctionSignature("trim_suffix", new List<string>{"s: string", "suffix: string"}, "string", "Trim suffix"),
                new FunctionSignature("replace", new List<string>{"s: string", "old: string", "new: string"}, "string", "Replace occurrences"),
                new FunctionSignature("split", new List<string>{"sep: string", "s: string"}, "list(string)", "Split string"),
                new FunctionSignature("join", new List<string>{"items: list", "sep: string"}, "string", "Join list to string"),
                new FunctionSignature("starts_with", new List<string>{"s: string", "prefix: string"}, "bool", "Check prefix"),
                new FunctionSignature("ends_with", new List<string>{"s: string", "suffix: string"}, "bool", "Check suffix"),
                new FunctionSignature("contains", new List<string>{"s: string", "sub: string"}, "bool", "Check containment"),
                new FunctionSignature("length", new List<string>{"s: string"}, "int", "String length"),
                new FunctionSignature("substr", new List<string>{"s: string", "start: int", "len: int"}, "string", "Substring"),
                new FunctionSignature("format", new List<string>{"fmt: string", "args: any"}, "string", "Format string"),
                new FunctionSignature("regex_match", new List<string>{"input: string", "pattern: string"}, "bool", "Regex match"),
                new FunctionSignature("regex_capture", new List<string>{"input: string", "pattern: string"}, "list(string)", "Regex capture"),
                new FunctionSignature("abs", new List<string>{"n: int"}, "int", "Absolute value"),
                new FunctionSignature("min", new List<string>{"a: int", "b: int"}, "int", "Minimum"),
                new FunctionSignature("max", new List<string>{"a: int", "b: int"}, "int", "Maximum"),
                new FunctionSignature("floor", new List<string>{"n: float"}, "int", "Floor"),
                new FunctionSignature("ceil", new List<string>{"n: float"}, "int", "Ceiling"),
                new FunctionSignature("round", new List<string>{"n: float"}, "int", "Round"),
                new FunctionSignature("sqrt", new List<string>{"n: float"}, "float", "Square root"),
                new FunctionSignature("pow", new List<string>{"base: float", "exp: float"}, "float", "Power"),
                new FunctionSignature("len", new List<string>{"col: any"}, "int", "Collection length"),
                new FunctionSignature("keys", new List<string>{"m: map"}, "list(string)", "Map keys"),
                new FunctionSignature("values", new List<string>{"m: map"}, "list", "Map values"),
                new FunctionSignature("flatten", new List<string>{"l: list"}, "list", "Flatten nested lists"),
                new FunctionSignature("concat", new List<string>{"lists: list"}, "list", "Concatenate lists"),
                new FunctionSignature("distinct", new List<string>{"l: list"}, "list", "Remove duplicates"),
                new FunctionSignature("sort", new List<string>{"l: list"}, "list", "Sort list"),
                new FunctionSignature("reverse", new List<string>{"l: list"}, "list", "Reverse list"),
                new FunctionSignature("index_of", new List<string>{"l: list", "item: any"}, "int", "Find index"),
                new FunctionSignature("range", new List<string>{"start: int", "end: int"}, "list(int)", "Generate range"),
                new FunctionSignature("zip", new List<string>{"a: list", "b: list"}, "list", "Zip two lists"),
                new FunctionSignature("sum", new List<string>{"l: list"}, "int", "Sum list"),
                new FunctionSignature("avg", new List<string>{"l: list"}, "float", "Average list"),
                new FunctionSignature("min_of", new List<string>{"l: list"}, "any", "Min of list"),
                new FunctionSignature("max_of", new List<string>{"l: list"}, "any", "Max of list"),
                new FunctionSignature("sha256", new List<string>{"s: string"}, "string", "SHA-256 hash"),
                new FunctionSignature("base64_encode", new List<string>{"s: string"}, "string", "Base64 encode"),
                new FunctionSignature("base64_decode", new List<string>{"s: string"}, "string", "Base64 decode"),
                new FunctionSignature("json_encode", new List<string>{"v: any"}, "string", "JSON encode"),
                new FunctionSignature("to_string", new List<string>{"v: any"}, "string", "Convert to string"),
                new FunctionSignature("to_int", new List<string>{"v: any"}, "int", "Convert to int"),
                new FunctionSignature("to_float", new List<string>{"v: any"}, "float", "Convert to float"),
                new FunctionSignature("to_bool", new List<string>{"v: any"}, "bool", "Convert to bool"),
                new FunctionSignature("type_of", new List<string>{"v: any"}, "string", "Get type name"),
                new FunctionSignature("has", new List<string>{"m: map", "key: string"}, "bool", "Check key exists"),
                new FunctionSignature("has_decorator", new List<string>{"block: block_ref", "name: string"}, "bool", "Check decorator"),
                new FunctionSignature("map", new List<string>{"l: list", "fn: function"}, "list", "Map over list"),
                new FunctionSignature("filter", new List<string>{"l: list", "fn: function"}, "list", "Filter list"),
                new FunctionSignature("every", new List<string>{"l: list", "fn: function"}, "bool", "All match"),
                new FunctionSignature("some", new List<string>{"l: list", "fn: function"}, "bool", "Any match"),
                new FunctionSignature("reduce", new List<string>{"l: list", "init: any", "fn: function"}, "any", "Reduce list"),
                new FunctionSignature("count", new List<string>{"l: list", "fn: function"}, "int", "Count matches"),
            };
        }
    }
}
