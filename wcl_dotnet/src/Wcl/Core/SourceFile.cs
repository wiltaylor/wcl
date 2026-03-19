using System.Collections.Generic;

namespace Wcl.Core
{
    public class SourceFile
    {
        public FileId Id { get; }
        public string Path { get; }
        public string Source { get; }
        private readonly List<int> _lineStarts;

        public SourceFile(FileId id, string path, string source)
        {
            Id = id;
            Path = path;
            Source = source;
            _lineStarts = ComputeLineStarts(source);
        }

        private static List<int> ComputeLineStarts(string source)
        {
            var starts = new List<int> { 0 };
            for (int i = 0; i < source.Length; i++)
            {
                if (source[i] == '\n')
                    starts.Add(i + 1);
            }
            return starts;
        }

        public (int Line, int Col) LineCol(int offset)
        {
            int lo = 0, hi = _lineStarts.Count - 1;
            while (lo < hi)
            {
                int mid = (lo + hi + 1) / 2;
                if (_lineStarts[mid] <= offset)
                    lo = mid;
                else
                    hi = mid - 1;
            }
            return (lo + 1, offset - _lineStarts[lo] + 1);
        }
    }
}
