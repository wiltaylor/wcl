import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.WclDocument;
import io.github.wiltaylor.wcl.eval.WclValue;
import io.github.wiltaylor.wcl.eval.WclValueKind;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public class Example {
    public static void main(String[] args) throws IOException {
        // Read the shared config file
        Path configPath = Path.of("../config/app.wcl");
        String source = Files.readString(configPath);

        // Parse the source
        try (WclDocument doc = Wcl.parse(source)) {

            // Check for errors
            if (doc.hasErrors()) {
                System.out.println("Parse errors:");
                for (var err : doc.getErrors()) {
                    System.out.println("  - " + err.message());
                }
                System.exit(1);
            }

            System.out.println("Parsed successfully!");

            // Count server blocks
            var servers = doc.getBlocksOfType("server");
            System.out.println("Server blocks: " + servers.size());

            // Print server names and ports
            System.out.println("\nServers:");
            var values = doc.getValues();
            var serverVal = values.get("server");
            if (serverVal != null && serverVal.getKind() == WclValueKind.MAP) {
                for (var entry : serverVal.asMap().entrySet()) {
                    var name = entry.getKey();
                    var attrs = entry.getValue();
                    var port = "?";
                    if (attrs.getKind() == WclValueKind.MAP) {
                        var portVal = attrs.asMap().get("port");
                        if (portVal != null) port = portVal.toString();
                    }
                    System.out.println("  " + name + ": port " + port);
                }
            }

            // Query for servers with workers > 2
            System.out.println("\nQuery: server | .workers > 2");
            try {
                var result = doc.query("server | .workers > 2");
                System.out.println("  Result: " + result);
            } catch (Exception e) {
                System.out.println("  Query error: " + e.getMessage());
            }

            // Print the users table
            System.out.println("\nUsers table:");
            var usersVal = values.get("users");
            if (usersVal != null && usersVal.getKind() == WclValueKind.LIST) {
                for (var row : usersVal.asList()) {
                    if (row.getKind() == WclValueKind.MAP) {
                        var map = row.asMap();
                        var name = map.containsKey("name") ? map.get("name").toString() : "";
                        var role = map.containsKey("role") ? map.get("role").toString() : "";
                        var admin = map.containsKey("admin") ? map.get("admin").toString() : "";
                        System.out.println("  " + name + " | " + role + " | admin=" + admin);
                    }
                }
            }
        }
    }
}
