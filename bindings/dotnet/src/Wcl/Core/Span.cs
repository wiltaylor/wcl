using System;

namespace Wcl.Core
{
    public readonly struct Span : IEquatable<Span>
    {
        public FileId File { get; }
        public int Start { get; }
        public int End { get; }

        public Span(FileId file, int start, int end)
        {
            File = file;
            Start = start;
            End = end;
        }

        public static Span Dummy() => new Span(new FileId(0), 0, 0);

        public Span Merge(Span other)
        {
            var start = Math.Min(Start, other.Start);
            var end = Math.Max(End, other.End);
            return new Span(File, start, end);
        }

        public int Length => End - Start;

        public bool Equals(Span other) =>
            File == other.File && Start == other.Start && End == other.End;

        public override bool Equals(object? obj) => obj is Span other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(File, Start, End);
        public override string ToString() => $"{File}:{Start}..{End}";

        public static bool operator ==(Span left, Span right) => left.Equals(right);
        public static bool operator !=(Span left, Span right) => !left.Equals(right);
    }
}
