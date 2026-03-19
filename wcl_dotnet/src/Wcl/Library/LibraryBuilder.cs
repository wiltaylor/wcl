using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Wcl.Eval.Functions;

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

        public FunctionSignature ToSignature()
        {
            return new FunctionSignature(
                Name,
                Params.Select(p => $"{p.Name}: {p.Type}").ToList(),
                ReturnType ?? "any",
                Doc ?? "");
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

        public void Install(string? targetDir = null)
        {
            var dir = targetDir ?? LibraryManager.UserLibraryDir();
            Directory.CreateDirectory(dir);
            var path = Path.Combine(dir, $"{_name}.wcl");
            File.WriteAllText(path, Build());
        }
    }

    public static class LibraryManager
    {
        public static string UserLibraryDir()
        {
            var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME")
                ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "share");
            return Path.Combine(dataHome, "wcl", "libraries");
        }

        public static void Install(string name, string content)
        {
            var dir = UserLibraryDir();
            Directory.CreateDirectory(dir);
            File.WriteAllText(Path.Combine(dir, $"{name}.wcl"), content);
        }

        public static bool Uninstall(string name)
        {
            var path = Path.Combine(UserLibraryDir(), $"{name}.wcl");
            if (File.Exists(path)) { File.Delete(path); return true; }
            return false;
        }

        public static List<string> List()
        {
            var dir = UserLibraryDir();
            if (!Directory.Exists(dir)) return new List<string>();
            return new List<string>(
                Directory.GetFiles(dir, "*.wcl")
                    .Select(f => Path.GetFileNameWithoutExtension(f)));
        }
    }
}
