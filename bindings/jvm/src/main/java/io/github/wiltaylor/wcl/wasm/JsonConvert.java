package io.github.wiltaylor.wcl.wasm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import io.github.wiltaylor.wcl.core.Diagnostic;
import io.github.wiltaylor.wcl.eval.*;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;

public final class JsonConvert {
    public static final ObjectMapper MAPPER = new ObjectMapper();

    private JsonConvert() {}

    public static WclValue toWclValue(JsonNode node) {
        if (node == null || node.isNull()) return WclValue.NULL;

        if (node.isBoolean()) return WclValue.ofBool(node.booleanValue());
        if (node.isTextual()) return WclValue.ofString(node.textValue());
        if (node.isNumber()) {
            if (node.canConvertToLong() && !node.isFloatingPointNumber()) {
                return WclValue.ofInt(node.longValue());
            }
            return WclValue.ofFloat(node.doubleValue());
        }

        if (node.isArray()) {
            var list = new ArrayList<WclValue>();
            for (var item : node) list.add(toWclValue(item));
            return WclValue.ofList(list);
        }

        if (node.isObject()) {
            // Check for set encoding
            var typeNode = node.get("__type");
            if (typeNode != null && "set".equals(typeNode.textValue())) {
                var itemsNode = node.get("items");
                if (itemsNode != null && itemsNode.isArray()) {
                    var setItems = new ArrayList<WclValue>();
                    for (var item : itemsNode) setItems.add(toWclValue(item));
                    return WclValue.ofSet(setItems);
                }
            }
            // Check for block ref encoding (has "kind" key)
            if (node.has("kind")) {
                return WclValue.ofBlockRef(toBlockRef(node));
            }
            // Regular map
            var map = new LinkedHashMap<String, WclValue>();
            var it = node.fields();
            while (it.hasNext()) {
                var entry = it.next();
                map.put(entry.getKey(), toWclValue(entry.getValue()));
            }
            return WclValue.ofMap(map);
        }

        return WclValue.NULL;
    }

    public static BlockRef toBlockRef(JsonNode node) {
        var kind = node.get("kind").textValue();

        String id = null;
        var idNode = node.get("id");
        if (idNode != null && idNode.isTextual()) id = idNode.textValue();

        var labels = new ArrayList<String>();
        var labelsNode = node.get("labels");
        if (labelsNode != null && labelsNode.isArray()) {
            for (var l : labelsNode) labels.add(l.textValue());
        }

        var attributes = new LinkedHashMap<String, WclValue>();
        var attrsNode = node.get("attributes");
        if (attrsNode != null && attrsNode.isObject()) {
            var it = attrsNode.fields();
            while (it.hasNext()) {
                var entry = it.next();
                attributes.put(entry.getKey(), toWclValue(entry.getValue()));
            }
        }

        var children = new ArrayList<BlockRef>();
        var childrenNode = node.get("children");
        if (childrenNode != null && childrenNode.isArray()) {
            for (var child : childrenNode) children.add(toBlockRef(child));
        }

        var decorators = new ArrayList<DecoratorValue>();
        var decsNode = node.get("decorators");
        if (decsNode != null && decsNode.isArray()) {
            for (var dec : decsNode) {
                var decName = dec.get("name").textValue();
                var args = new LinkedHashMap<String, WclValue>();
                var argsNode = dec.get("args");
                if (argsNode != null && argsNode.isObject()) {
                    var ait = argsNode.fields();
                    while (ait.hasNext()) {
                        var entry = ait.next();
                        args.put(entry.getKey(), toWclValue(entry.getValue()));
                    }
                }
                decorators.add(new DecoratorValue(decName, args));
            }
        }

        return new BlockRef(kind, id, labels, attributes, children, decorators);
    }

    public static Diagnostic toDiagnostic(JsonNode node) {
        var severity = node.get("severity").textValue();
        var message = node.get("message").textValue();
        String code = null;
        var codeNode = node.get("code");
        if (codeNode != null && codeNode.isTextual()) code = codeNode.textValue();
        return new Diagnostic(severity, message, code);
    }

    public static LinkedHashMap<String, WclValue> toValues(JsonNode node) {
        var map = new LinkedHashMap<String, WclValue>();
        var it = node.fields();
        while (it.hasNext()) {
            var entry = it.next();
            map.put(entry.getKey(), toWclValue(entry.getValue()));
        }
        return map;
    }

    public static String wclValueToJson(WclValue value) {
        return switch (value.getKind()) {
            case NULL -> "null";
            case BOOL -> value.asBool() ? "true" : "false";
            case INT -> Long.toString(value.asInt());
            case FLOAT -> Double.toString(value.asFloat());
            case STRING -> {
                try {
                    yield MAPPER.writeValueAsString(value.asString());
                } catch (Exception e) {
                    yield "\"\"";
                }
            }
            case LIST -> {
                var items = value.asList();
                var parts = new String[items.size()];
                for (int i = 0; i < items.size(); i++) parts[i] = wclValueToJson(items.get(i));
                yield "[" + String.join(",", parts) + "]";
            }
            case MAP -> {
                var map = value.asMap();
                var parts = new ArrayList<String>(map.size());
                for (var entry : map.entrySet()) {
                    try {
                        parts.add(MAPPER.writeValueAsString(entry.getKey()) + ":" + wclValueToJson(entry.getValue()));
                    } catch (Exception e) {
                        // skip
                    }
                }
                yield "{" + String.join(",", parts) + "}";
            }
            case SET -> {
                var items = value.asSet();
                var parts = new String[items.size()];
                for (int i = 0; i < items.size(); i++) parts[i] = wclValueToJson(items.get(i));
                yield "{\"__type\":\"set\",\"items\":[" + String.join(",", parts) + "]}";
            }
            case BLOCK_REF -> blockRefToJson(value.asBlockRef());
        };
    }

    private static String blockRefToJson(BlockRef br) {
        var parts = new ArrayList<String>();
        try {
            parts.add("\"kind\":" + MAPPER.writeValueAsString(br.getKind()));
            if (br.getId() != null)
                parts.add("\"id\":" + MAPPER.writeValueAsString(br.getId()));
            if (!br.getLabels().isEmpty()) {
                var labelParts = new String[br.getLabels().size()];
                for (int i = 0; i < br.getLabels().size(); i++)
                    labelParts[i] = MAPPER.writeValueAsString(br.getLabels().get(i));
                parts.add("\"labels\":[" + String.join(",", labelParts) + "]");
            }
            if (!br.getAttributes().isEmpty()) {
                var attrParts = new ArrayList<String>(br.getAttributes().size());
                for (var entry : br.getAttributes().entrySet())
                    attrParts.add(MAPPER.writeValueAsString(entry.getKey()) + ":" + wclValueToJson(entry.getValue()));
                parts.add("\"attributes\":{" + String.join(",", attrParts) + "}");
            }
            if (!br.getChildren().isEmpty()) {
                var childParts = new String[br.getChildren().size()];
                for (int i = 0; i < br.getChildren().size(); i++)
                    childParts[i] = blockRefToJson(br.getChildren().get(i));
                parts.add("\"children\":[" + String.join(",", childParts) + "]");
            }
            if (!br.getDecorators().isEmpty()) {
                var decParts = new String[br.getDecorators().size()];
                for (int i = 0; i < br.getDecorators().size(); i++) {
                    var d = br.getDecorators().get(i);
                    var argParts = new ArrayList<String>(d.args().size());
                    for (var entry : d.args().entrySet())
                        argParts.add(MAPPER.writeValueAsString(entry.getKey()) + ":" + wclValueToJson(entry.getValue()));
                    decParts[i] = "{\"name\":" + MAPPER.writeValueAsString(d.name()) +
                            ",\"args\":{" + String.join(",", argParts) + "}}";
                }
                parts.add("\"decorators\":[" + String.join(",", decParts) + "]");
            }
        } catch (Exception e) {
            // fallback
        }
        return "{" + String.join(",", parts) + "}";
    }
}
