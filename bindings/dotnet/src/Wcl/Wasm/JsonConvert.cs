using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text.Json;
using Wcl.Core;
using Wcl.Eval;

namespace Wcl.Wasm
{
    internal static class JsonConvert
    {
        internal static WclValue ToWclValue(JsonElement el)
        {
            switch (el.ValueKind)
            {
                case JsonValueKind.Null:
                case JsonValueKind.Undefined:
                    return WclValue.Null;
                case JsonValueKind.True:
                    return WclValue.NewBool(true);
                case JsonValueKind.False:
                    return WclValue.NewBool(false);
                case JsonValueKind.String:
                    return WclValue.NewString(el.GetString()!);
                case JsonValueKind.Number:
                    if (el.TryGetInt64(out var i))
                        return WclValue.NewInt(i);
                    return WclValue.NewFloat(el.GetDouble());
                case JsonValueKind.Array:
                    var list = new List<WclValue>();
                    foreach (var item in el.EnumerateArray())
                        list.Add(ToWclValue(item));
                    return WclValue.NewList(list);
                case JsonValueKind.Object:
                    // Check for set encoding
                    if (el.TryGetProperty("__type", out var typeEl) &&
                        typeEl.GetString() == "set" &&
                        el.TryGetProperty("items", out var itemsEl))
                    {
                        var setItems = new List<WclValue>();
                        foreach (var item in itemsEl.EnumerateArray())
                            setItems.Add(ToWclValue(item));
                        return WclValue.NewSet(setItems);
                    }
                    // Check for block ref encoding (has "kind" key)
                    if (el.TryGetProperty("kind", out _))
                    {
                        return WclValue.NewBlockRef(ToBlockRef(el));
                    }
                    // Regular map
                    var map = new OrderedMap<string, WclValue>();
                    foreach (var prop in el.EnumerateObject())
                        map[prop.Name] = ToWclValue(prop.Value);
                    return WclValue.NewMap(map);
                default:
                    return WclValue.Null;
            }
        }

        internal static BlockRef ToBlockRef(JsonElement el)
        {
            var kind = el.GetProperty("kind").GetString()!;

            string? id = null;
            if (el.TryGetProperty("id", out var idEl) && idEl.ValueKind == JsonValueKind.String)
                id = idEl.GetString();

            var attributes = new OrderedMap<string, WclValue>();
            if (el.TryGetProperty("attributes", out var attrsEl))
            {
                foreach (var prop in attrsEl.EnumerateObject())
                    attributes[prop.Name] = ToWclValue(prop.Value);
            }

            var children = new List<BlockRef>();
            if (el.TryGetProperty("children", out var childrenEl))
            {
                foreach (var child in childrenEl.EnumerateArray())
                    children.Add(ToBlockRef(child));
            }

            var decorators = new List<DecoratorValue>();
            if (el.TryGetProperty("decorators", out var decsEl))
            {
                foreach (var dec in decsEl.EnumerateArray())
                {
                    var decName = dec.GetProperty("name").GetString()!;
                    var args = new OrderedMap<string, WclValue>();
                    if (dec.TryGetProperty("args", out var argsEl))
                    {
                        foreach (var prop in argsEl.EnumerateObject())
                            args[prop.Name] = ToWclValue(prop.Value);
                    }
                    decorators.Add(new DecoratorValue(decName, args));
                }
            }

            return new BlockRef(kind, id, attributes, children, decorators);
        }

        internal static Diagnostic ToDiagnostic(JsonElement el)
        {
            var severity = el.GetProperty("severity").GetString()!;
            var message = el.GetProperty("message").GetString()!;
            string? code = null;
            if (el.TryGetProperty("code", out var codeEl) && codeEl.ValueKind == JsonValueKind.String)
                code = codeEl.GetString();
            return new Diagnostic(severity, message, code);
        }

        internal static OrderedMap<string, WclValue> ToValues(JsonElement el)
        {
            var map = new OrderedMap<string, WclValue>();
            foreach (var prop in el.EnumerateObject())
                map[prop.Name] = ToWclValue(prop.Value);
            return map;
        }

        internal static string WclValueToJson(WclValue value)
        {
            switch (value.Kind)
            {
                case WclValueKind.Null:
                    return "null";
                case WclValueKind.Bool:
                    return value.AsBool() ? "true" : "false";
                case WclValueKind.Int:
                    return value.AsInt().ToString(CultureInfo.InvariantCulture);
                case WclValueKind.Float:
                    return value.AsFloat().ToString(CultureInfo.InvariantCulture);
                case WclValueKind.String:
                    return JsonSerializer.Serialize(value.AsString());
                case WclValueKind.List:
                {
                    var items = value.AsList();
                    var parts = new string[items.Count];
                    for (int i = 0; i < items.Count; i++)
                        parts[i] = WclValueToJson(items[i]);
                    return "[" + string.Join(",", parts) + "]";
                }
                case WclValueKind.Map:
                {
                    var map = value.AsMap();
                    var parts = new List<string>(map.Count);
                    foreach (var kvp in map)
                        parts.Add(JsonSerializer.Serialize(kvp.Key) + ":" + WclValueToJson(kvp.Value));
                    return "{" + string.Join(",", parts) + "}";
                }
                case WclValueKind.Set:
                {
                    var items = value.AsSet();
                    var parts = new string[items.Count];
                    for (int i = 0; i < items.Count; i++)
                        parts[i] = WclValueToJson(items[i]);
                    return "{\"__type\":\"set\",\"items\":[" + string.Join(",", parts) + "]}";
                }
                case WclValueKind.BlockRef:
                {
                    var br = value.AsBlockRef();
                    return BlockRefToJson(br);
                }
                default:
                    return "null";
            }
        }

        private static string BlockRefToJson(BlockRef br)
        {
            var parts = new List<string>();
            parts.Add("\"kind\":" + JsonSerializer.Serialize(br.Kind));
            if (br.Id != null)
                parts.Add("\"id\":" + JsonSerializer.Serialize(br.Id));
            if (br.Attributes.Count > 0)
            {
                var attrParts = new List<string>(br.Attributes.Count);
                foreach (var kvp in br.Attributes)
                    attrParts.Add(JsonSerializer.Serialize(kvp.Key) + ":" + WclValueToJson(kvp.Value));
                parts.Add("\"attributes\":{" + string.Join(",", attrParts) + "}");
            }
            if (br.Children.Count > 0)
            {
                var childParts = new string[br.Children.Count];
                for (int i = 0; i < br.Children.Count; i++)
                    childParts[i] = BlockRefToJson(br.Children[i]);
                parts.Add("\"children\":[" + string.Join(",", childParts) + "]");
            }
            if (br.Decorators.Count > 0)
            {
                var decParts = new string[br.Decorators.Count];
                for (int i = 0; i < br.Decorators.Count; i++)
                {
                    var d = br.Decorators[i];
                    var argParts = new List<string>(d.Args.Count);
                    foreach (var kvp in d.Args)
                        argParts.Add(JsonSerializer.Serialize(kvp.Key) + ":" + WclValueToJson(kvp.Value));
                    decParts[i] = "{\"name\":" + JsonSerializer.Serialize(d.Name) +
                                  ",\"args\":{" + string.Join(",", argParts) + "}}";
                }
                parts.Add("\"decorators\":[" + string.Join(",", decParts) + "]");
            }
            return "{" + string.Join(",", parts) + "}";
        }
    }
}
