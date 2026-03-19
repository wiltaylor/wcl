using System;
using System.Collections.Generic;
using System.IO;
using Wcl.Core;
using Wcl.Core.Ast;

namespace Wcl.Eval.Import
{
    public interface IFileSystem
    {
        string? ReadFile(string path);
        string Canonicalize(string path);
        bool Exists(string path);
    }

    public class RealFileSystem : IFileSystem
    {
        public string? ReadFile(string path)
        {
            try { return File.ReadAllText(path); }
            catch { return null; }
        }
        public string Canonicalize(string path) => Path.GetFullPath(path);
        public bool Exists(string path) => File.Exists(path);
    }

    public class InMemoryFileSystem : IFileSystem
    {
        private readonly Dictionary<string, string> _files = new Dictionary<string, string>();

        public void AddFile(string path, string content) => _files[path] = content;

        public string? ReadFile(string path) =>
            _files.TryGetValue(path, out var content) ? content : null;
        public string Canonicalize(string path) => path;
        public bool Exists(string path) => _files.ContainsKey(path);
    }

    public class ImportResolver
    {
        private readonly IFileSystem _fs;
        private readonly SourceMap _sourceMap;
        private readonly string _rootDir;
        private readonly uint _maxDepth;
        private readonly bool _allowImports;
        private readonly HashSet<string> _resolved = new HashSet<string>();
        private readonly List<string> _librarySearchPaths = new List<string>();
        private readonly HashSet<string> _exportedNames = new HashSet<string>();

        public ImportResolver(IFileSystem fs, SourceMap sourceMap, string rootDir,
                              uint maxDepth, bool allowImports)
        {
            _fs = fs;
            _sourceMap = sourceMap;
            _rootDir = rootDir;
            _maxDepth = maxDepth;
            _allowImports = allowImports;

            // XDG library paths
            var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME")
                ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "share");
            _librarySearchPaths.Add(Path.Combine(dataHome, "wcl", "libraries"));

            // System library paths
            if (!System.Runtime.InteropServices.RuntimeInformation.IsOSPlatform(
                    System.Runtime.InteropServices.OSPlatform.Windows))
            {
                _librarySearchPaths.Add("/usr/local/share/wcl/libraries");
                _librarySearchPaths.Add("/usr/share/wcl/libraries");
            }
        }

        public DiagnosticBag Resolve(Document doc, string currentFile, uint depth)
        {
            var diags = new DiagnosticBag();
            if (!_allowImports) return diags;
            if (depth > _maxDepth)
            {
                diags.ErrorWithCode("E014", $"import depth exceeded (max {_maxDepth})", Span.Dummy());
                return diags;
            }

            var newItems = new List<DocItem>();
            foreach (var item in doc.Items)
            {
                if (item is ImportItem imp)
                {
                    var path = ResolveImportPath(imp.Import, currentFile, diags);
                    if (path == null) continue;

                    if (_resolved.Contains(path)) continue; // dedup
                    _resolved.Add(path);

                    var source = _fs.ReadFile(path);
                    if (source == null)
                    {
                        diags.ErrorWithCode("E010", $"could not read import: {path}", imp.Import.Span);
                        continue;
                    }

                    var fileId = _sourceMap.AddFile(path, source);
                    var (importDoc, parseDiags) = Core.Parser.WclParser.Parse(source, fileId);
                    diags.Merge(parseDiags);

                    var subDiags = Resolve(importDoc, path, depth + 1);
                    diags.Merge(subDiags);

                    // Check for duplicate exported names (E034)
                    foreach (var importItem in importDoc.Items)
                    {
                        if (importItem is ExportLetItem eli)
                        {
                            if (_exportedNames.Contains(eli.ExportLet.Name.Name))
                            {
                                diags.ErrorWithCode("E034",
                                    $"duplicate exported variable '{eli.ExportLet.Name.Name}' across imports",
                                    eli.ExportLet.Span);
                            }
                            _exportedNames.Add(eli.ExportLet.Name.Name);
                        }
                    }

                    newItems.AddRange(importDoc.Items);
                }
                else
                {
                    newItems.Add(item);
                }
            }
            doc.Items = newItems;
            return diags;
        }

        private string? ResolveImportPath(Core.Ast.Import import, string currentFile, DiagnosticBag diags)
        {
            var pathStr = "";
            foreach (var part in import.Path.Parts)
            {
                if (part is LiteralPart lp) pathStr += lp.Value;
            }

            if (import.Kind == ImportKind.Library)
            {
                foreach (var searchPath in _librarySearchPaths)
                {
                    var fullPath = Path.Combine(searchPath, pathStr);
                    if (_fs.Exists(fullPath)) return _fs.Canonicalize(fullPath);
                }
                diags.ErrorWithCode("E015", $"library not found: {pathStr}", import.Span);
                return null;
            }

            // Reject absolute paths
            if (Path.IsPathRooted(pathStr) || pathStr.StartsWith("~"))
            {
                diags.ErrorWithCode("E013", $"absolute imports are not allowed: {pathStr}", import.Span);
                return null;
            }

            // Reject remote URLs
            if (pathStr.StartsWith("http://") || pathStr.StartsWith("https://"))
            {
                diags.ErrorWithCode("E013", $"remote imports are not allowed: {pathStr}", import.Span);
                return null;
            }

            // Relative import
            var dir = Path.GetDirectoryName(currentFile) ?? _rootDir;
            var resolved = _fs.Canonicalize(Path.Combine(dir, pathStr));

            // Jail check (E011)
            var canonicalRoot = _fs.Canonicalize(_rootDir);
            if (!resolved.StartsWith(canonicalRoot))
            {
                diags.ErrorWithCode("E011",
                    $"import escapes root directory: {pathStr}", import.Span);
                return null;
            }

            return resolved;
        }
    }
}
