package io.github.wiltaylor.wcl.serde;

import java.lang.reflect.Modifier;
import java.util.List;
import java.util.Locale;
import java.util.Map;

public final class WclSerializer {
    private WclSerializer() {}

    public static String serialize(Object value, boolean pretty) {
        var sb = new StringBuilder();
        serializeObject(value, sb, pretty, 0);
        return sb.toString();
    }

    private static void serializeObject(Object value, StringBuilder sb, boolean pretty, int indent) {
        if (value == null) { sb.append("null"); return; }

        var type = value.getClass();

        if (type == String.class) { sb.append('"').append(escapeString((String) value)).append('"'); return; }
        if (type == Integer.class || type == Long.class) { sb.append(value); return; }
        if (type == Double.class) { sb.append(String.format(Locale.ROOT, "%s", value)); return; }
        if (type == Float.class) { sb.append(String.format(Locale.ROOT, "%s", value)); return; }
        if (type == Boolean.class) { sb.append((Boolean) value ? "true" : "false"); return; }

        // List
        if (value instanceof List<?> list) {
            sb.append('[');
            for (int i = 0; i < list.size(); i++) {
                if (i > 0) sb.append(", ");
                serializeObject(list.get(i), sb, pretty, indent);
            }
            sb.append(']');
            return;
        }

        // Map
        if (value instanceof Map<?, ?> map) {
            if (pretty) serializeMapPretty(map, sb, indent);
            else serializeMapCompact(map, sb);
            return;
        }

        // POJO
        var fields = type.getDeclaredFields();
        if (pretty) {
            var inner = " ".repeat(indent + 4);
            sb.append('\n');
            for (var field : fields) {
                if (Modifier.isStatic(field.getModifiers())) continue;
                field.setAccessible(true);
                try {
                    var val = field.get(value);
                    sb.append(inner).append(WclDeserializer.toSnakeCase(field.getName())).append(" = ");
                    serializeObject(val, sb, pretty, indent + 4);
                    sb.append('\n');
                } catch (IllegalAccessException e) {
                    // skip
                }
            }
        } else {
            boolean first = true;
            for (var field : fields) {
                if (Modifier.isStatic(field.getModifiers())) continue;
                field.setAccessible(true);
                try {
                    if (!first) sb.append('\n');
                    first = false;
                    var val = field.get(value);
                    sb.append(WclDeserializer.toSnakeCase(field.getName())).append(" = ");
                    serializeObject(val, sb, false, 0);
                } catch (IllegalAccessException e) {
                    // skip
                }
            }
        }
    }

    private static void serializeMapCompact(Map<?, ?> map, StringBuilder sb) {
        sb.append('{');
        boolean first = true;
        for (var entry : map.entrySet()) {
            if (!first) sb.append(", ");
            first = false;
            sb.append(entry.getKey()).append(" = ");
            serializeObject(entry.getValue(), sb, false, 0);
        }
        sb.append('}');
    }

    private static void serializeMapPretty(Map<?, ?> map, StringBuilder sb, int indent) {
        var inner = " ".repeat(indent + 4);
        sb.append("{\n");
        for (var entry : map.entrySet()) {
            sb.append(inner).append(entry.getKey()).append(" = ");
            serializeObject(entry.getValue(), sb, true, indent + 4);
            sb.append('\n');
        }
        sb.append(" ".repeat(indent)).append('}');
    }

    private static String escapeString(String s) {
        return s.replace("\\", "\\\\").replace("\"", "\\\"")
                .replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t");
    }
}
