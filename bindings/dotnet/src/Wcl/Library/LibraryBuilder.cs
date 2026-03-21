using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

namespace Wcl.Library
{
    public class FunctionStub
    {
        public string Name { get; set; }
        public List<(string Name, string Type)> Params { get; set; }
        public string? ReturnType { get; set; }
        public string? Doc { get; set; }

        public FunctionStub(string name, List<(string, string)> parms, string? returnType = null, string? doc = null)
        {
            Name = name; Params = parms; ReturnType = returnType; Doc = doc;
        }

        public string ToWcl()
        {
            var paramStr = string.Join(", ", Params.Select(p => $"{p.Name}: {p.Type}"));
            var ret = ReturnType != null ? $" -> {ReturnType}" : "";
            return $"declare {Name}({paramStr}){ret}\n";
        }
    }

    public class LibraryBuilder
    {
        private readonly string _name;
        private readonly List<string> _schemaTexts = new List<string>();
        private readonly List<FunctionStub> _stubs = new List<FunctionStub>();

        public LibraryBuilder(string name) { _name = name; }

        public void AddSchemaText(string text) => _schemaTexts.Add(text);
        public void AddFunctionStub(FunctionStub stub) => _stubs.Add(stub);

        public string Build()
        {
            var sb = new StringBuilder();
            foreach (var schema in _schemaTexts)
            {
                sb.Append(schema);
                if (!schema.EndsWith("\n")) sb.AppendLine();
            }
            foreach (var stub in _stubs)
            {
                sb.Append(stub.ToWcl());
            }
            return sb.ToString();
        }

        public string Install()
        {
            return LibraryManager.Install(_name, Build());
        }
    }

    public static class LibraryManager
    {
        private static string GetLibraryDir()
        {
            var xdgDataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
            if (!string.IsNullOrEmpty(xdgDataHome))
                return Path.Combine(xdgDataHome, "wcl", "lib");

            var home = Environment.GetEnvironmentVariable("HOME");
            if (!string.IsNullOrEmpty(home))
                return Path.Combine(home, ".local", "share", "wcl", "lib");

            return Path.Combine(".wcl", "lib");
        }

        public static string Install(string name, string content)
        {
            var dir = GetLibraryDir();
            Directory.CreateDirectory(dir);
            var path = Path.Combine(dir, name);
            File.WriteAllText(path, content);
            return path;
        }

        public static void Uninstall(string name)
        {
            var dir = GetLibraryDir();
            var path = Path.Combine(dir, name);
            File.Delete(path);
        }

        public static List<string> List()
        {
            var dir = GetLibraryDir();
            if (!Directory.Exists(dir))
                return new List<string>();

            return Directory.GetFiles(dir, "*.wcl")
                .OrderBy(p => p)
                .ToList();
        }
    }
}
