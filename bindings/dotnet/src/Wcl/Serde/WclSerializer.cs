using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Text;
using Wcl.Core;
using Wcl.Eval;

namespace Wcl.Serde
{
    public static class WclSerializer
    {
        public static string Serialize(object value, bool pretty = false)
        {
            var sb = new StringBuilder();
            SerializeObject(value, sb, pretty, 0);
            return sb.ToString();
        }

        private static void SerializeObject(object? value, StringBuilder sb, bool pretty, int indent)
        {
            if (value == null) { sb.Append("null"); return; }

            var type = value.GetType();

            if (type == typeof(string)) { sb.Append('"').Append(EscapeString((string)value)).Append('"'); return; }
            if (type == typeof(int) || type == typeof(long) || type == typeof(Int64))
            { sb.Append(value); return; }
            if (type == typeof(double)) { sb.Append(((double)value).ToString(CultureInfo.InvariantCulture)); return; }
            if (type == typeof(float)) { sb.Append(((float)value).ToString(CultureInfo.InvariantCulture)); return; }
            if (type == typeof(bool)) { sb.Append((bool)value ? "true" : "false"); return; }

            // List/Array
            if (value is IList list)
            {
                sb.Append('[');
                for (int i = 0; i < list.Count; i++)
                {
                    if (i > 0) sb.Append(", ");
                    SerializeObject(list[i], sb, pretty, indent);
                }
                sb.Append(']');
                return;
            }

            // Dictionary
            if (value is IDictionary dict)
            {
                if (pretty) SerializeMapPretty(dict, sb, indent);
                else SerializeMapCompact(dict, sb);
                return;
            }

            // POCO
            var props = type.GetProperties(BindingFlags.Public | BindingFlags.Instance);
            if (pretty)
            {
                var ind = new string(' ', indent);
                var inner = new string(' ', indent + 4);
                sb.AppendLine();
                foreach (var prop in props)
                {
                    if (!prop.CanRead) continue;
                    var val = prop.GetValue(value);
                    sb.Append(inner).Append(ToSnakeCase(prop.Name)).Append(" = ");
                    SerializeObject(val, sb, pretty, indent + 4);
                    sb.AppendLine();
                }
            }
            else
            {
                bool first = true;
                foreach (var prop in props)
                {
                    if (!prop.CanRead) continue;
                    if (!first) sb.AppendLine();
                    first = false;
                    var val = prop.GetValue(value);
                    sb.Append(ToSnakeCase(prop.Name)).Append(" = ");
                    SerializeObject(val, sb, false, 0);
                }
            }
        }

        private static void SerializeMapCompact(IDictionary dict, StringBuilder sb)
        {
            sb.Append('{');
            bool first = true;
            foreach (DictionaryEntry entry in dict)
            {
                if (!first) sb.Append(", ");
                first = false;
                sb.Append(entry.Key).Append(" = ");
                SerializeObject(entry.Value, sb, false, 0);
            }
            sb.Append('}');
        }

        private static void SerializeMapPretty(IDictionary dict, StringBuilder sb, int indent)
        {
            var inner = new string(' ', indent + 4);
            sb.AppendLine("{");
            foreach (DictionaryEntry entry in dict)
            {
                sb.Append(inner).Append(entry.Key).Append(" = ");
                SerializeObject(entry.Value, sb, true, indent + 4);
                sb.AppendLine();
            }
            sb.Append(new string(' ', indent)).Append('}');
        }

        private static string EscapeString(string s)
        {
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"")
                    .Replace("\n", "\\n").Replace("\r", "\\r").Replace("\t", "\\t");
        }

        private static string ToSnakeCase(string name)
        {
            var sb = new StringBuilder();
            for (int i = 0; i < name.Length; i++)
            {
                if (char.IsUpper(name[i]))
                {
                    if (i > 0) sb.Append('_');
                    sb.Append(char.ToLower(name[i]));
                }
                else sb.Append(name[i]);
            }
            return sb.ToString();
        }
    }
}
