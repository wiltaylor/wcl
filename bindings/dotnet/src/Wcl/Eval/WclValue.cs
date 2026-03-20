using System;
using System.Collections.Generic;
using System.Text;
using Wcl.Core;

namespace Wcl.Eval
{
    public enum WclValueKind
    {
        String, Int, Float, Bool, Null,
        List, Map, Set, BlockRef
    }

    public class WclValue : IEquatable<WclValue>
    {
        public WclValueKind Kind { get; }

        private readonly string? _stringValue;
        private readonly long _intValue;
        private readonly double _floatValue;
        private readonly bool _boolValue;
        private readonly List<WclValue>? _listValue;
        private readonly OrderedMap<string, WclValue>? _mapValue;
        private readonly BlockRef? _blockRef;

        private WclValue(WclValueKind kind, string? s = null, long i = 0, double d = 0,
                         bool b = false, List<WclValue>? list = null,
                         OrderedMap<string, WclValue>? map = null,
                         BlockRef? br = null)
        {
            Kind = kind;
            _stringValue = s;
            _intValue = i;
            _floatValue = d;
            _boolValue = b;
            _listValue = list;
            _mapValue = map;
            _blockRef = br;
        }

        // Factory methods
        public static WclValue NewString(string value) => new WclValue(WclValueKind.String, s: value);
        public static WclValue NewInt(long value) => new WclValue(WclValueKind.Int, i: value);
        public static WclValue NewFloat(double value) => new WclValue(WclValueKind.Float, d: value);
        public static WclValue NewBool(bool value) => new WclValue(WclValueKind.Bool, b: value);
        public static readonly WclValue Null = new WclValue(WclValueKind.Null);
        public static WclValue NewList(List<WclValue> items) => new WclValue(WclValueKind.List, list: items);
        public static WclValue NewMap(OrderedMap<string, WclValue> map) => new WclValue(WclValueKind.Map, map: map);
        public static WclValue NewSet(List<WclValue> items) => new WclValue(WclValueKind.Set, list: items);
        public static WclValue NewBlockRef(BlockRef blockRef) => new WclValue(WclValueKind.BlockRef, br: blockRef);

        // Accessors
        public string AsString() => Kind == WclValueKind.String ? _stringValue! : throw new InvalidOperationException($"expected string, got {TypeName}");
        public long AsInt() => Kind == WclValueKind.Int ? _intValue : throw new InvalidOperationException($"expected int, got {TypeName}");
        public double AsFloat() => Kind == WclValueKind.Float ? _floatValue : throw new InvalidOperationException($"expected float, got {TypeName}");
        public bool AsBool() => Kind == WclValueKind.Bool ? _boolValue : throw new InvalidOperationException($"expected bool, got {TypeName}");
        public List<WclValue> AsList() => Kind == WclValueKind.List ? _listValue! : throw new InvalidOperationException($"expected list, got {TypeName}");
        public OrderedMap<string, WclValue> AsMap() => Kind == WclValueKind.Map ? _mapValue! : throw new InvalidOperationException($"expected map, got {TypeName}");
        public List<WclValue> AsSet() => Kind == WclValueKind.Set ? _listValue! : throw new InvalidOperationException($"expected set, got {TypeName}");
        public BlockRef AsBlockRef() => Kind == WclValueKind.BlockRef ? _blockRef! : throw new InvalidOperationException($"expected block_ref, got {TypeName}");

        // Try accessors
        public string? TryAsString() => Kind == WclValueKind.String ? _stringValue : null;
        public long? TryAsInt() => Kind == WclValueKind.Int ? _intValue : (long?)null;
        public double? TryAsFloat() => Kind == WclValueKind.Float ? _floatValue : (double?)null;
        public bool? TryAsBool() => Kind == WclValueKind.Bool ? _boolValue : (bool?)null;
        public List<WclValue>? TryAsList() => Kind == WclValueKind.List ? _listValue : null;
        public OrderedMap<string, WclValue>? TryAsMap() => Kind == WclValueKind.Map ? _mapValue : null;
        public BlockRef? TryAsBlockRef() => Kind == WclValueKind.BlockRef ? _blockRef : null;

        public bool IsNull => Kind == WclValueKind.Null;

        public bool? IsTruthy() => Kind == WclValueKind.Bool ? _boolValue : (bool?)null;

        public string TypeName => Kind switch
        {
            WclValueKind.String => "string",
            WclValueKind.Int => "int",
            WclValueKind.Float => "float",
            WclValueKind.Bool => "bool",
            WclValueKind.Null => "null",
            WclValueKind.List => "list",
            WclValueKind.Map => "map",
            WclValueKind.Set => "set",
            WclValueKind.BlockRef => "block_ref",
            _ => "unknown",
        };

        public string ToInterpString()
        {
            switch (Kind)
            {
                case WclValueKind.String: return _stringValue!;
                case WclValueKind.Int: return _intValue.ToString();
                case WclValueKind.Float: return _floatValue.ToString(System.Globalization.CultureInfo.InvariantCulture);
                case WclValueKind.Bool: return _boolValue ? "true" : "false";
                case WclValueKind.Null: return "null";
                default: throw new InvalidOperationException($"cannot interpolate {TypeName} into string");
            }
        }

        public bool Equals(WclValue? other)
        {
            if (other is null) return false;
            if (Kind != other.Kind) return false;
            switch (Kind)
            {
                case WclValueKind.String: return _stringValue == other._stringValue;
                case WclValueKind.Int: return _intValue == other._intValue;
                case WclValueKind.Float: return _floatValue == other._floatValue;
                case WclValueKind.Bool: return _boolValue == other._boolValue;
                case WclValueKind.Null: return true;
                case WclValueKind.List:
                case WclValueKind.Set:
                {
                    if (_listValue!.Count != other._listValue!.Count) return false;
                    for (int i = 0; i < _listValue.Count; i++)
                        if (!_listValue[i].Equals(other._listValue[i])) return false;
                    return true;
                }
                case WclValueKind.Map:
                {
                    if (_mapValue!.Count != other._mapValue!.Count) return false;
                    for (int i = 0; i < _mapValue.Count; i++)
                    {
                        var a = _mapValue.GetAt(i);
                        var b = other._mapValue.GetAt(i);
                        if (a.Key != b.Key || !a.Value.Equals(b.Value)) return false;
                    }
                    return true;
                }
                default: return false;
            }
        }

        public override bool Equals(object? obj) => obj is WclValue other && Equals(other);
        public override int GetHashCode() => Kind.GetHashCode();

        public static bool operator ==(WclValue? left, WclValue? right)
        {
            if (left is null) return right is null;
            return left.Equals(right);
        }
        public static bool operator !=(WclValue? left, WclValue? right) => !(left == right);

        public override string ToString()
        {
            switch (Kind)
            {
                case WclValueKind.String: return _stringValue!;
                case WclValueKind.Int: return _intValue.ToString();
                case WclValueKind.Float: return _floatValue.ToString(System.Globalization.CultureInfo.InvariantCulture);
                case WclValueKind.Bool: return _boolValue ? "true" : "false";
                case WclValueKind.Null: return "null";
                case WclValueKind.List:
                {
                    var sb = new StringBuilder("[");
                    for (int i = 0; i < _listValue!.Count; i++)
                    {
                        if (i > 0) sb.Append(", ");
                        sb.Append(_listValue[i]);
                    }
                    sb.Append(']');
                    return sb.ToString();
                }
                case WclValueKind.Map:
                {
                    var sb = new StringBuilder("{");
                    int idx = 0;
                    foreach (var kvp in _mapValue!)
                    {
                        if (idx++ > 0) sb.Append(", ");
                        sb.Append(kvp.Key).Append(" = ").Append(kvp.Value);
                    }
                    sb.Append('}');
                    return sb.ToString();
                }
                case WclValueKind.Set:
                {
                    var sb = new StringBuilder("set(");
                    for (int i = 0; i < _listValue!.Count; i++)
                    {
                        if (i > 0) sb.Append(", ");
                        sb.Append(_listValue[i]);
                    }
                    sb.Append(')');
                    return sb.ToString();
                }
                case WclValueKind.BlockRef:
                {
                    var br = _blockRef!;
                    var sb = new StringBuilder(br.Kind);
                    if (br.Id != null) sb.Append(' ').Append(br.Id);
                    sb.Append(" {");
                    int idx = 0;
                    foreach (var kvp in br.Attributes)
                    {
                        if (idx++ > 0) sb.Append(',');
                        sb.Append(' ').Append(kvp.Key).Append(" = ").Append(kvp.Value);
                    }
                    sb.Append(" }");
                    return sb.ToString();
                }
                default: return Kind.ToString();
            }
        }
    }
}
