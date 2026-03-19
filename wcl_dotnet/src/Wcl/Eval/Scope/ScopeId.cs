using System;

namespace Wcl.Eval
{
    public readonly struct ScopeId : IEquatable<ScopeId>
    {
        public uint Value { get; }
        public ScopeId(uint value) => Value = value;
        public bool Equals(ScopeId other) => Value == other.Value;
        public override bool Equals(object? obj) => obj is ScopeId other && Equals(other);
        public override int GetHashCode() => (int)Value;
        public static bool operator ==(ScopeId a, ScopeId b) => a.Equals(b);
        public static bool operator !=(ScopeId a, ScopeId b) => !a.Equals(b);
    }
}
