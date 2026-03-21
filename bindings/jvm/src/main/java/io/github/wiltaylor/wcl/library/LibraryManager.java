package io.github.wiltaylor.wcl.library;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class LibraryManager {
    private LibraryManager() {}

    private static Path getLibraryDir() {
        var xdgDataHome = System.getenv("XDG_DATA_HOME");
        if (xdgDataHome != null && !xdgDataHome.isEmpty()) {
            return Path.of(xdgDataHome, "wcl", "lib");
        }
        var home = System.getenv("HOME");
        if (home != null && !home.isEmpty()) {
            return Path.of(home, ".local", "share", "wcl", "lib");
        }
        return Path.of(".wcl", "lib");
    }

    public static List<String> list() {
        var dir = getLibraryDir();
        if (!Files.isDirectory(dir)) return Collections.emptyList();
        try (Stream<Path> paths = Files.list(dir)) {
            return paths
                    .filter(p -> p.toString().endsWith(".wcl"))
                    .map(Path::toString)
                    .sorted()
                    .collect(Collectors.toList());
        } catch (IOException e) {
            return Collections.emptyList();
        }
    }
}
