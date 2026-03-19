using System;

namespace Wcl.Core
{
    public readonly struct FileId : IEquatable<FileId>
    {
        public uint Value { get; }

        public FileId(uint value) => Value = value;

        public bool Equals(FileId other) => Value == other.Value;
        public override bool Equals(object? obj) => obj is FileId other && Equals(other);
        public override int GetHashCode() => (int)Value;
        public override string ToString() => $"FileId({Value})";

        public static bool operator ==(FileId left, FileId right) => left.Equals(right);
        public static bool operator !=(FileId left, FileId right) => !left.Equals(right);
    }
}
