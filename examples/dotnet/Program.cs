using System;
using System.IO;
using Wcl;
using Wcl.Eval;

class Program
{
    static void Main()
    {
        // Read the shared config file
        var configPath = Path.Combine(Directory.GetCurrentDirectory(), "..", "config", "app.wcl");
        var source = File.ReadAllText(configPath);

        // Parse the source
        using var doc = WclParser.Parse(source);

        // Check for errors
        if (doc.HasErrors())
        {
            Console.WriteLine("Parse errors:");
            foreach (var err in doc.Errors())
                Console.WriteLine($"  - {err.Message}");
            Environment.Exit(1);
        }

        Console.WriteLine("Parsed successfully!");

        // Count server blocks
        var servers = doc.BlocksOfType("server");
        Console.WriteLine($"Server blocks: {servers.Count}");

        // Print server names and ports
        Console.WriteLine("\nServers:");
        if (doc.Values.TryGetValue("server", out var serverVal) && serverVal.Kind == WclValueKind.Map)
        {
            foreach (var (name, attrs) in serverVal.AsMap())
            {
                var port = attrs.Kind == WclValueKind.Map && attrs.AsMap().ContainsKey("port")
                    ? attrs.AsMap()["port"].ToString()
                    : "?";
                Console.WriteLine($"  {name}: port {port}");
            }
        }

        // Query for servers with workers > 2
        Console.WriteLine("\nQuery: server | .workers > 2");
        try
        {
            var result = doc.Query("server | .workers > 2");
            Console.WriteLine($"  Result: {result}");
        }
        catch (Exception e)
        {
            Console.WriteLine($"  Query error: {e.Message}");
        }

        // Print the users table
        Console.WriteLine("\nUsers table:");
        if (doc.Values.TryGetValue("users", out var usersVal) && usersVal.Kind == WclValueKind.List)
        {
            foreach (var row in usersVal.AsList())
            {
                if (row.Kind == WclValueKind.Map)
                {
                    var map = row.AsMap();
                    var name = map.ContainsKey("name") ? map["name"].ToString() : "";
                    var role = map.ContainsKey("role") ? map["role"].ToString() : "";
                    var admin = map.ContainsKey("admin") ? map["admin"].ToString() : "";
                    Console.WriteLine($"  {name} | {role} | admin={admin}");
                }
            }
        }
    }
}
