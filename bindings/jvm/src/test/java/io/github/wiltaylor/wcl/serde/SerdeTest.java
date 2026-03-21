package io.github.wiltaylor.wcl.serde;

import io.github.wiltaylor.wcl.eval.WclValue;
import org.junit.jupiter.api.Test;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

class SerdeTest {
    @Test
    void deserializePrimitives() {
        assertEquals("hello", WclDeserializer.fromValue(WclValue.ofString("hello"), String.class));
        assertEquals(42L, WclDeserializer.fromValue(WclValue.ofInt(42), Long.class));
        assertEquals(3.14, WclDeserializer.fromValue(WclValue.ofFloat(3.14), Double.class));
        assertEquals(true, WclDeserializer.fromValue(WclValue.ofBool(true), Boolean.class));
    }

    @Test
    void deserializeList() {
        var val = WclValue.ofList(List.of(WclValue.ofInt(1), WclValue.ofInt(2), WclValue.ofInt(3)));
        @SuppressWarnings("unchecked")
        var result = (List<Object>) WclDeserializer.fromValue(val, List.class);
        assertEquals(3, result.size());
        assertEquals(1L, result.get(0));
    }

    @Test
    void deserializeDict() {
        var map = new LinkedHashMap<String, WclValue>();
        map.put("a", WclValue.ofInt(1));
        map.put("b", WclValue.ofInt(2));
        @SuppressWarnings("unchecked")
        var result = (Map<String, Object>) WclDeserializer.fromValue(WclValue.ofMap(map), Map.class);
        assertEquals(1L, result.get("a"));
        assertEquals(2L, result.get("b"));
    }

    @Test
    void deserializeIntToFloat() {
        assertEquals(42.0, WclDeserializer.fromValue(WclValue.ofInt(42), Double.class));
    }

    @Test
    void deserializeNullToReference() {
        assertNull(WclDeserializer.fromValue(WclValue.NULL, String.class));
    }

    @Test
    void serializeCompact() {
        var map = new LinkedHashMap<String, Object>();
        map.put("name", "test");
        map.put("count", 42);
        var result = WclSerializer.serialize(map, false);
        assertTrue(result.contains("name"));
    }

    @Test
    void serializeString() {
        assertEquals("\"hello\"", WclSerializer.serialize("hello", false));
    }

    @Test
    void serializeBool() {
        assertEquals("true", WclSerializer.serialize(true, false));
        assertEquals("false", WclSerializer.serialize(false, false));
    }

    @Test
    void serializeNull() {
        assertEquals("null", WclSerializer.serialize(null, false));
    }
}
