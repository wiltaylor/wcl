using System.Collections.Generic;
using System.Linq;
using Wcl.Core;
using Wcl.Core.Lexer;
using Wcl.Core.Tokens;
using Xunit;

namespace Wcl.Tests.Core
{
    public class LexerTests
    {
        private static List<Token> Lex(string source)
        {
            var (tokens, diags) = WclLexer.Lex(source, new FileId(0));
            Assert.False(diags.HasErrors, $"Lex errors: {string.Join(", ", diags.IntoDiagnostics().ConvertAll(d => d.Message))}");
            return tokens;
        }

        private static List<TokenKind> LexKinds(string source) =>
            Lex(source).Select(t => t.Kind).ToList();

        [Fact]
        public void Keywords()
        {
            var kinds = LexKinds("let partial macro schema table import export query ref for in if else");
            Assert.Equal(TokenKind.Let, kinds[0]);
            Assert.Equal(TokenKind.Partial, kinds[1]);
            Assert.Equal(TokenKind.Macro, kinds[2]);
            Assert.Equal(TokenKind.Schema, kinds[3]);
            Assert.Equal(TokenKind.Table, kinds[4]);
            Assert.Equal(TokenKind.Import, kinds[5]);
            Assert.Equal(TokenKind.Export, kinds[6]);
            Assert.Equal(TokenKind.Query, kinds[7]);
            Assert.Equal(TokenKind.Ref, kinds[8]);
            Assert.Equal(TokenKind.For, kinds[9]);
            Assert.Equal(TokenKind.In, kinds[10]);
            Assert.Equal(TokenKind.If, kinds[11]);
            Assert.Equal(TokenKind.Else, kinds[12]);
            Assert.Equal(TokenKind.Eof, kinds[13]);
        }

        [Fact]
        public void BoolAndNull()
        {
            var tokens = Lex("true false null");
            Assert.Equal(TokenKind.BoolLit, tokens[0].Kind);
            Assert.True(tokens[0].BoolValue);
            Assert.Equal(TokenKind.BoolLit, tokens[1].Kind);
            Assert.False(tokens[1].BoolValue);
            Assert.Equal(TokenKind.NullLit, tokens[2].Kind);
        }

        [Fact]
        public void PlainIdentifier()
        {
            var tokens = Lex("my_var _private camelCase");
            Assert.Equal(TokenKind.Ident, tokens[0].Kind);
            Assert.Equal("my_var", tokens[0].StringValue);
            Assert.Equal("_private", tokens[1].StringValue);
            Assert.Equal("camelCase", tokens[2].StringValue);
        }

        [Fact]
        public void IdentifierLitWithHyphens()
        {
            var tokens = Lex("svc-payments node-01");
            Assert.Equal(TokenKind.IdentifierLit, tokens[0].Kind);
            Assert.Equal("svc-payments", tokens[0].StringValue);
            Assert.Equal("node-01", tokens[1].StringValue);
        }

        [Fact]
        public void HyphenAtEndIsSeparateMinus()
        {
            var kinds = LexKinds("foo-");
            Assert.Equal(TokenKind.Ident, kinds[0]);
            Assert.Equal(TokenKind.Minus, kinds[1]);
        }

        [Fact]
        public void DecimalIntegers()
        {
            var tokens = Lex("0 42 1000");
            Assert.Equal(0L, tokens[0].IntValue);
            Assert.Equal(42L, tokens[1].IntValue);
            Assert.Equal(1000L, tokens[2].IntValue);
        }

        [Fact]
        public void DecimalWithUnderscores()
        {
            var tokens = Lex("1_000_000");
            Assert.Equal(1000000L, tokens[0].IntValue);
        }

        [Fact]
        public void HexLiterals()
        {
            var tokens = Lex("0xFF 0x00");
            Assert.Equal(255L, tokens[0].IntValue);
            Assert.Equal(0L, tokens[1].IntValue);
        }

        [Fact]
        public void OctalLiterals()
        {
            var tokens = Lex("0o755 0o0");
            Assert.Equal(493L, tokens[0].IntValue); // 0o755
            Assert.Equal(0L, tokens[1].IntValue);
        }

        [Fact]
        public void BinaryLiterals()
        {
            var tokens = Lex("0b1010 0b0");
            Assert.Equal(10L, tokens[0].IntValue);
            Assert.Equal(0L, tokens[1].IntValue);
        }

        [Fact]
        public void FloatLiterals()
        {
            var tokens = Lex("3.14 1.0e10");
            Assert.Equal(TokenKind.FloatLit, tokens[0].Kind);
            Assert.Equal(3.14, tokens[0].DoubleValue);
            Assert.Equal(1.0e10, tokens[1].DoubleValue);
        }

        [Fact]
        public void EmptyString()
        {
            var tokens = Lex("\"\"");
            Assert.Equal(TokenKind.StringLit, tokens[0].Kind);
            Assert.Equal("", tokens[0].StringValue);
        }

        [Fact]
        public void SimpleString()
        {
            var tokens = Lex("\"hello world\"");
            Assert.Equal("hello world", tokens[0].StringValue);
        }

        [Fact]
        public void StringEscapes()
        {
            var tokens = Lex("\"\\n\\t\\r\\\\\\\"\"");
            Assert.Equal("\n\t\r\\\"", tokens[0].StringValue);
        }

        [Fact]
        public void StringUnicodeEscape4()
        {
            var tokens = Lex("\"\\u0041\"");
            Assert.Equal("A", tokens[0].StringValue);
        }

        [Fact]
        public void Operators()
        {
            var kinds = LexKinds("== != <= >= =~ && || =>");
            Assert.Equal(TokenKind.EqEq, kinds[0]);
            Assert.Equal(TokenKind.Neq, kinds[1]);
            Assert.Equal(TokenKind.Lte, kinds[2]);
            Assert.Equal(TokenKind.Gte, kinds[3]);
            Assert.Equal(TokenKind.Match, kinds[4]);
            Assert.Equal(TokenKind.And, kinds[5]);
            Assert.Equal(TokenKind.Or, kinds[6]);
            Assert.Equal(TokenKind.FatArrow, kinds[7]);
        }

        [Fact]
        public void NestedBlockComments()
        {
            var tokens = Lex("/* outer /* inner */ still outer */ 42");
            Assert.Equal(TokenKind.BlockComment, tokens[0].Kind);
            Assert.Equal(TokenKind.IntLit, tokens[1].Kind);
            Assert.Equal(42L, tokens[1].IntValue);
        }

        [Fact]
        public void DeclareKeyword()
        {
            var kinds = LexKinds("declare my_fn");
            Assert.Equal(TokenKind.Declare, kinds[0]);
            Assert.Equal(TokenKind.Ident, kinds[1]);
        }

        [Fact]
        public void InterpolationPreserved()
        {
            var tokens = Lex("\"hello ${name}\"");
            Assert.Contains("${", tokens[0].StringValue);
        }
    }
}
