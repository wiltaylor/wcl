package io.github.wiltaylor.wcl.serde;

import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.eval.WclValueKind;

import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Type;
import java.util.*;

public final class WclDeserializer {
    private WclDeserializer() {}

    @SuppressWarnings("unchecked")
    public static <T> T fromValue(WclValue value, Class<T> targetType) {
        return (T) convertValue(value, targetType, null);
    }

    @SuppressWarnings("unchecked")
    private static Object convertValue(WclValue value, Class<?> rawType, Type genericType) {
        if (value.isNull()) {
            if (rawType.isPrimitive()) {
                throw new SerdeException("cannot assign null to " + rawType.getName());
            }
            return null;
        }

        // Primitives and wrappers
        if (rawType == String.class) {
            if (value.getKind() == WclValueKind.DATE) return value.asDate();
            if (value.getKind() == WclValueKind.DURATION) return value.asDuration();
            return value.asString();
        }
        if (rawType == long.class || rawType == Long.class) {
            if (value.getKind() == WclValueKind.BIG_INT) return value.asBigInt();
            return value.asInt();
        }
        if (rawType == int.class || rawType == Integer.class) {
            if (value.getKind() == WclValueKind.BIG_INT) return (int) value.asBigInt();
            return (int) value.asInt();
        }
        if (rawType == double.class || rawType == Double.class) {
            return value.getKind() == WclValueKind.INT ? (double) value.asInt() : value.asFloat();
        }
        if (rawType == float.class || rawType == Float.class) {
            return value.getKind() == WclValueKind.INT ? (float) value.asInt() : (float) value.asFloat();
        }
        if (rawType == boolean.class || rawType == Boolean.class) return value.asBool();

        // WclValue passthrough
        if (rawType == WclValue.class) return value;

        // Object.class - auto-convert based on WclValue kind
        if (rawType == Object.class) {
            return switch (value.getKind()) {
                case STRING -> value.asString();
                case INT -> value.asInt();
                case FLOAT -> value.asFloat();
                case BOOL -> value.asBool();
                case NULL -> null;
                case LIST -> {
                    var items = value.asList();
                    var list = new ArrayList<>(items.size());
                    for (var item : items) list.add(convertValue(item, Object.class, null));
                    yield list;
                }
                case MAP -> {
                    var srcMap = value.asMap();
                    var result = new LinkedHashMap<String, Object>();
                    for (var entry : srcMap.entrySet())
                        result.put(entry.getKey(), convertValue(entry.getValue(), Object.class, null));
                    yield result;
                }
                case SET -> {
                    var items = value.asSet();
                    var list = new ArrayList<>(items.size());
                    for (var item : items) list.add(convertValue(item, Object.class, null));
                    yield list;
                }
                case BIG_INT -> value.asBigInt();
                case DATE -> value.asDate();
                case DURATION -> value.asDuration();
                case BLOCK_REF -> value.asBlockRef();
            };
        }

        // List<T>
        if (List.class.isAssignableFrom(rawType)) {
            Class<?> itemType = Object.class;
            if (genericType instanceof ParameterizedType pt) {
                var ta = pt.getActualTypeArguments()[0];
                if (ta instanceof Class<?> c) itemType = c;
            }
            var items = value.asList();
            var list = new ArrayList<>(items.size());
            for (var item : items) list.add(convertValue(item, itemType, null));
            return list;
        }

        // BlockRef -> Map with id auto-populated
        if (value.getKind() == WclValueKind.BLOCK_REF) {
            var br = value.asBlockRef();
            var map = new LinkedHashMap<String, WclValue>();
            if (br.getId() != null) map.put("id", WclValue.ofString(br.getId()));
            map.putAll(br.getAttributes());
            return convertValue(WclValue.ofMap(map), rawType, genericType);
        }

        // Set -> List coercion
        if (value.getKind() == WclValueKind.SET && List.class.isAssignableFrom(rawType)) {
            return convertValue(WclValue.ofList(value.asSet()), rawType, genericType);
        }

        // Map<String, T>
        if (Map.class.isAssignableFrom(rawType)) {
            Class<?> valType = Object.class;
            if (genericType instanceof ParameterizedType pt && pt.getActualTypeArguments().length >= 2) {
                var ta = pt.getActualTypeArguments()[1];
                if (ta instanceof Class<?> c) valType = c;
            }
            var srcMap = value.asMap();
            var result = new LinkedHashMap<>();
            for (var entry : srcMap.entrySet()) {
                result.put(entry.getKey(), convertValue(entry.getValue(), valType, null));
            }
            return result;
        }

        // POJO via reflection
        if (value.getKind() == WclValueKind.MAP) {
            var map = value.asMap();
            try {
                var obj = rawType.getDeclaredConstructor().newInstance();
                for (var field : rawType.getDeclaredFields()) {
                    if (java.lang.reflect.Modifier.isStatic(field.getModifiers())) continue;
                    field.setAccessible(true);
                    var snakeName = toSnakeCase(field.getName());
                    WclValue val = map.get(snakeName);
                    if (val == null) val = map.get(field.getName());
                    if (val != null) {
                        field.set(obj, convertValue(val, field.getType(), field.getGenericType()));
                    } else if (field.getType().isPrimitive()) {
                        throw new SerdeException("missing required field: " + snakeName);
                    }
                }
                return obj;
            } catch (SerdeException e) {
                throw e;
            } catch (Exception e) {
                throw new SerdeException("failed to deserialize into " + rawType.getName() + ": " + e.getMessage());
            }
        }

        throw new SerdeException("cannot deserialize " + value.typeName() + " into " + rawType.getName());
    }

    static String toSnakeCase(String name) {
        var sb = new StringBuilder();
        for (int i = 0; i < name.length(); i++) {
            var ch = name.charAt(i);
            if (Character.isUpperCase(ch)) {
                if (i > 0) sb.append('_');
                sb.append(Character.toLowerCase(ch));
            } else {
                sb.append(ch);
            }
        }
        return sb.toString();
    }
}
