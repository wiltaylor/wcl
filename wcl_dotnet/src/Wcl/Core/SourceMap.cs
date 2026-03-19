using System.Collections.Generic;

namespace Wcl.Core
{
    public class SourceMap
    {
        private readonly List<SourceFile> _files = new List<SourceFile>();

        public FileId AddFile(string path, string source)
        {
            var id = new FileId((uint)_files.Count);
            _files.Add(new SourceFile(id, path, source));
            return id;
        }

        public SourceFile? GetFile(FileId id)
        {
            int idx = (int)id.Value;
            if (idx >= 0 && idx < _files.Count)
                return _files[idx];
            return null;
        }

        public int FileCount => _files.Count;
    }
}
