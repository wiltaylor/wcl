using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Wcl.Core;
using Wcl.Eval;

namespace Wcl.Serde
{
    public class SerdeError : Exception
    {
        public SerdeError(string message) : base(message) { }

        public static SerdeError TypeMismatch(string expected, string actual) =>
            new SerdeError($"type mismatch: expected {expected}, got {actual}");
        public static SerdeError MissingField(string name) =>
            new SerdeError($"missing required field: {name}");
    }

    public static class WclDeserializer
    {
        public static T FromValue<T>(WclValue value)
        {
            return (T)ConvertValue(value, typeof(T))!;
        }

        private static object? ConvertValue(WclValue value, Type targetType)
        {
            // Handle nullable
            var underlying = Nullable.GetUnderlyingType(targetType);
            if (underlying != null)
            {
                if (value.IsNull) return null;
                return ConvertValue(value, underlying);
            }

            // Null
            if (value.IsNull)
            {
                if (!targetType.IsValueType) return null;
                throw new SerdeError($"cannot assign null to {targetType.Name}");
            }

            // Primitives
            if (targetType == typeof(string))
            {
                return value.AsString();
            }
            if (targetType == typeof(long) || targetType == typeof(Int64)) return value.AsInt();
            if (targetType == typeof(int) || targetType == typeof(Int32)) return (int)value.AsInt();
            if (targetType == typeof(double)) return value.Kind == WclValueKind.Int ? (double)value.AsInt() : value.AsFloat();
            if (targetType == typeof(float)) return value.Kind == WclValueKind.Int ? (float)value.AsInt() : (float)value.AsFloat();
            if (targetType == typeof(bool)) return value.AsBool();

            // List<T>
            if (targetType.IsGenericType && targetType.GetGenericTypeDefinition() == typeof(List<>))
            {
                var itemType = targetType.GetGenericArguments()[0];
                var list = (IList)Activator.CreateInstance(targetType)!;
                foreach (var item in value.AsList())
                    list.Add(ConvertValue(item, itemType));
                return list;
            }

            // BlockRef -> Map with id auto-populated
            if (value.Kind == WclValueKind.BlockRef)
            {
                var br = value.AsBlockRef();
                var map = new OrderedMap<string, WclValue>();
                if (br.Id != null)
                    map["id"] = WclValue.NewString(br.Id);
                foreach (var kvp in br.Attributes)
                    map[kvp.Key] = kvp.Value;
                return ConvertValue(WclValue.NewMap(map), targetType);
            }

            // Set -> List coercion
            if (value.Kind == WclValueKind.Set && targetType.IsGenericType &&
                targetType.GetGenericTypeDefinition() == typeof(List<>))
            {
                return ConvertValue(WclValue.NewList(value.AsSet()), targetType);
            }

            // WclValue passthrough
            if (targetType == typeof(WclValue)) return value;

            // Dictionary<string, T>
            if (targetType.IsGenericType && targetType.GetGenericTypeDefinition() == typeof(Dictionary<,>))
            {
                var valType = targetType.GetGenericArguments()[1];
                var dict = (IDictionary)Activator.CreateInstance(targetType)!;
                if (value.Kind == WclValueKind.Map)
                {
                    foreach (var kvp in value.AsMap())
                        dict.Add(kvp.Key, ConvertValue(kvp.Value, valType));
                }
                return dict;
            }

            // POCO via reflection
            if (value.Kind == WclValueKind.Map)
            {
                var map = value.AsMap();
                var obj = Activator.CreateInstance(targetType)!;
                foreach (var prop in targetType.GetProperties(BindingFlags.Public | BindingFlags.Instance))
                {
                    if (!prop.CanWrite) continue;
                    var name = prop.Name;
                    var snakeName = ToSnakeCase(name);
                    if (map.TryGetValue(snakeName, out var val) || map.TryGetValue(name, out val))
                    {
                        prop.SetValue(obj, ConvertValue(val, prop.PropertyType));
                    }
                    else if (!IsOptionalType(prop.PropertyType))
                    {
                        throw SerdeError.MissingField(snakeName);
                    }
                }
                foreach (var field in targetType.GetFields(BindingFlags.Public | BindingFlags.Instance))
                {
                    var name = field.Name;
                    var snakeName = ToSnakeCase(name);
                    if (map.TryGetValue(snakeName, out var val) || map.TryGetValue(name, out val))
                    {
                        field.SetValue(obj, ConvertValue(val, field.FieldType));
                    }
                    else if (!IsOptionalType(field.FieldType))
                    {
                        throw SerdeError.MissingField(snakeName);
                    }
                }
                return obj;
            }

            throw new SerdeError($"cannot deserialize {value.TypeName} into {targetType.Name}");
        }

        private static bool IsOptionalType(Type type) =>
            !type.IsValueType || Nullable.GetUnderlyingType(type) != null;

        private static string ToSnakeCase(string name)
        {
            var sb = new System.Text.StringBuilder();
            for (int i = 0; i < name.Length; i++)
            {
                if (char.IsUpper(name[i]))
                {
                    if (i > 0) sb.Append('_');
                    sb.Append(char.ToLower(name[i]));
                }
                else
                {
                    sb.Append(name[i]);
                }
            }
            return sb.ToString();
        }
    }
}
