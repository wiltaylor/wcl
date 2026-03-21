package io.github.wiltaylor.wcl;

import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.eval.WclValueKind;
import io.github.wiltaylor.wcl.library.LibraryManager;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Function;

import static org.junit.jupiter.api.Assertions.*;

class WclTest {
    @Test
    void parseSimpleKeyValue() {
        try (var doc = Wcl.parse("x = 42\ny = \"hello\"")) {
            assertFalse(doc.hasErrors());
            var values = doc.getValues();
            assertEquals(42L, values.get("x").asInt());
            assertEquals("hello", values.get("y").asString());
        }
    }

    @Test
    void parseWithErrors() {
        try (var doc = Wcl.parse("x = @invalid")) {
            assertTrue(doc.hasErrors());
            var errors = doc.getErrors();
            assertFalse(errors.isEmpty());
            assertEquals("error", errors.get(0).severity());
        }
    }

    @Test
    void parseFile() throws IOException {
        var dir = Files.createTempDirectory("wcl-test");
        var path = dir.resolve("test.wcl");
        Files.writeString(path, "port = 8080\nhost = \"localhost\"");

        try (var doc = Wcl.parseFile(path.toString())) {
            assertFalse(doc.hasErrors());
            assertEquals(8080L, doc.getValues().get("port").asInt());
            assertEquals("localhost", doc.getValues().get("host").asString());
        } finally {
            Files.deleteIfExists(path);
            Files.deleteIfExists(dir);
        }
    }

    @Test
    void parseFileNotFound() {
        assertThrows(WclException.class, () -> Wcl.parseFile("/nonexistent/path.wcl"));
    }

    @Test
    void queryExecution() {
        try (var doc = Wcl.parse("service { port = 8080 }\nservice { port = 9090 }")) {
            assertFalse(doc.hasErrors());
            var result = doc.query("service | .port");
            var ports = result.asList();
            assertEquals(2, ports.size());
            assertEquals(8080L, ports.get(0).asInt());
            assertEquals(9090L, ports.get(1).asInt());
        }
    }

    @Test
    void customFunctions() {
        var functions = new HashMap<String, Function<WclValue[], WclValue>>();
        functions.put("double", args -> WclValue.ofInt(args[0].asInt() * 2));

        var options = new ParseOptions().functions(functions);

        try (var doc = Wcl.parse("result = double(21)", options)) {
            assertFalse(doc.hasErrors());
            assertEquals(42L, doc.getValues().get("result").asInt());
        }
    }

    @Test
    void blocksAndBlocksOfType() {
        try (var doc = Wcl.parse("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }")) {
            assertFalse(doc.hasErrors());
            var blocks = doc.getBlocks();
            assertEquals(3, blocks.size());

            var servers = doc.getBlocksOfType("server");
            assertEquals(2, servers.size());
            assertEquals("server", servers.get(0).getKind());
        }
    }

    @Test
    void diagnosticsOnValidInput() {
        try (var doc = Wcl.parse("x = 42")) {
            var diags = doc.getDiagnostics();
            assertTrue(diags.stream().noneMatch(d -> d.isError()));
        }
    }

    @Test
    void libraryManagement() {
        var name = "test_jvm_lib.wcl";
        var content = "schema \"test_config\" {\n    port: int\n}\n";

        var path = LibraryManager.install(name, content);
        assertFalse(path.isEmpty());

        var libs = LibraryManager.list();
        assertTrue(libs.stream().anyMatch(lib -> lib.endsWith(name)));

        LibraryManager.uninstall(name);
    }

    @Test
    void documentClose() {
        var doc = Wcl.parse("x = 1");
        doc.close();
        // Double close should not throw
        doc.close();

        // Access after close should throw
        assertThrows(IllegalStateException.class, doc::getValues);
    }

    @Test
    void concurrentReads() throws InterruptedException {
        try (var doc = Wcl.parse("x = 42\ny = \"hello\"")) {
            int threadCount = 10;
            var latch = new CountDownLatch(threadCount);
            var error = new AtomicReference<Throwable>();

            for (int i = 0; i < threadCount; i++) {
                new Thread(() -> {
                    try {
                        var values = doc.getValues();
                        assertEquals(42L, values.get("x").asInt());
                    } catch (Throwable t) {
                        error.compareAndSet(null, t);
                    } finally {
                        latch.countDown();
                    }
                }).start();
            }
            latch.await();
            assertNull(error.get());
        }
    }

    @Test
    void fromString() {
        var result = Wcl.fromString("x = 10\ny = 20", Map.class);
        assertEquals(10L, result.get("x"));
        assertEquals(20L, result.get("y"));
    }

    @Test
    void blocksWithDecorators() {
        try (var doc = Wcl.parse("""
                @deprecated("use new-svc")
                server old-svc {
                    port = 80
                }
                """)) {
            assertFalse(doc.hasErrors());
            var blocks = doc.getBlocks();
            assertEquals(1, blocks.size());
            assertEquals("server", blocks.get(0).getKind());
            assertEquals("old-svc", blocks.get(0).getId());
            assertTrue(blocks.get(0).hasDecorator("deprecated"));
            assertNotNull(blocks.get(0).getDecorator("deprecated"));
        }
    }

    @Test
    void nestedBlocks() {
        try (var doc = Wcl.parse("""
                server main {
                    port = 8080
                    logging {
                        level = "info"
                    }
                }
                """)) {
            assertFalse(doc.hasErrors());
            var blocks = doc.getBlocks();
            assertEquals(1, blocks.size());
            assertEquals("server", blocks.get(0).getKind());
            assertFalse(blocks.get(0).getChildren().isEmpty());
            assertEquals("logging", blocks.get(0).getChildren().get(0).getKind());
        }
    }

    @Test
    void listValues() {
        try (var doc = Wcl.parse("tags = [\"a\", \"b\", \"c\"]")) {
            assertFalse(doc.hasErrors());
            var tags = doc.getValues().get("tags");
            assertEquals(WclValueKind.LIST, tags.getKind());
            assertEquals(3, tags.asList().size());
            assertEquals("a", tags.asList().get(0).asString());
        }
    }

    @Test
    void blockAttributes() {
        try (var doc = Wcl.parse("""
                server web {
                    port = 8080
                    host = "localhost"
                    debug = false
                }
                """)) {
            assertFalse(doc.hasErrors());
            var servers = doc.getBlocksOfType("server");
            assertEquals(1, servers.size());
            var s = servers.get(0);
            assertEquals("web", s.getId());
            assertEquals(WclValue.ofInt(8080), s.get("port"));
            assertEquals(WclValue.ofString("localhost"), s.get("host"));
            assertEquals(WclValue.ofBool(false), s.get("debug"));
            assertNull(s.get("nonexistent"));
        }
    }

    @Test
    void mapValues() {
        try (var doc = Wcl.parse("config = { a = 1, b = 2 }")) {
            assertFalse(doc.hasErrors());
            var config = doc.getValues().get("config");
            assertEquals(WclValueKind.MAP, config.getKind());
            var map = config.asMap();
            assertEquals(2, map.size());
            assertEquals(1L, map.get("a").asInt());
            assertEquals(2L, map.get("b").asInt());
        }
    }

    @Test
    void nullValues() {
        try (var doc = Wcl.parse("x = null")) {
            assertFalse(doc.hasErrors());
            assertTrue(doc.getValues().get("x").isNull());
        }
    }

    @Test
    void boolAndFloatValues() {
        try (var doc = Wcl.parse("flag = true\npi = 3.14")) {
            assertFalse(doc.hasErrors());
            assertTrue(doc.getValues().get("flag").asBool());
            assertEquals(3.14, doc.getValues().get("pi").asFloat());
        }
    }

    @Test
    void queryById() {
        try (var doc = Wcl.parse("""
                server api { port = 8080 }
                server web { port = 9090 }
                """)) {
            assertFalse(doc.hasErrors());
            var result = doc.query("server#api");
            var list = result.asList();
            assertEquals(1, list.size());
            var br = list.get(0).asBlockRef();
            assertEquals("api", br.getId());
        }
    }
}
