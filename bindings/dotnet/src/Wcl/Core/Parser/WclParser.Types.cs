using System.Collections.Generic;
using Wcl.Core.Ast;
using Wcl.Core.Tokens;

namespace Wcl.Core.Parser
{
    public partial class WclParser
    {
        internal TypeExpr? ParseTypeExpr()
        {
            SkipNewlines();

            // Handle ref keyword (not an ident)
            if (PeekKind() == TokenKind.Ref)
            {
                var s = CurrentSpan();
                Advance();
                Expect(TokenKind.LParen);
                var schemaName = ParseStringLit();
                if (schemaName == null) return null;
                Expect(TokenKind.RParen);
                return new RefTypeExpr(schemaName, s.Merge(PrevSpan()));
            }

            // Handle null literal as type
            if (PeekKind() == TokenKind.NullLit)
            {
                var s = CurrentSpan();
                Advance();
                return new NullTypeExpr(s);
            }

            if (PeekKind() != TokenKind.Ident)
            {
                _diagnostics.Error($"expected type expression, found {PeekKind()}", CurrentSpan());
                return null;
            }

            var name = Peek().StringValue!;
            var start = CurrentSpan();

            switch (name)
            {
                case "string": Advance(); return new StringTypeExpr(start);
                case "int": Advance(); return new IntTypeExpr(start);
                case "float": Advance(); return new FloatTypeExpr(start);
                case "bool": Advance(); return new BoolTypeExpr(start);
                case "null": Advance(); return new NullTypeExpr(start);
                case "identifier": Advance(); return new IdentifierTypeExpr(start);
                case "any": Advance(); return new AnyTypeExpr(start);
                case "list":
                {
                    Advance();
                    if (!At(TokenKind.LParen))
                        return new ListTypeExpr(new AnyTypeExpr(Span.Dummy()), start);
                    Advance();
                    var inner = ParseTypeExpr();
                    if (inner == null) return null;
                    Expect(TokenKind.RParen);
                    return new ListTypeExpr(inner, start.Merge(PrevSpan()));
                }
                case "map":
                {
                    Advance();
                    if (!At(TokenKind.LParen))
                        return new MapTypeExpr(new StringTypeExpr(Span.Dummy()), new AnyTypeExpr(Span.Dummy()), start);
                    Advance();
                    var keyType = ParseTypeExpr();
                    if (keyType == null) return null;
                    Expect(TokenKind.Comma);
                    SkipNewlines();
                    var valType = ParseTypeExpr();
                    if (valType == null) return null;
                    Expect(TokenKind.RParen);
                    return new MapTypeExpr(keyType, valType, start.Merge(PrevSpan()));
                }
                case "set":
                {
                    Advance();
                    if (!At(TokenKind.LParen))
                        return new SetTypeExpr(new AnyTypeExpr(Span.Dummy()), start);
                    Advance();
                    var inner = ParseTypeExpr();
                    if (inner == null) return null;
                    Expect(TokenKind.RParen);
                    return new SetTypeExpr(inner, start.Merge(PrevSpan()));
                }
                case "ref":
                {
                    Advance();
                    Expect(TokenKind.LParen);
                    var schemaName = ParseStringLit();
                    if (schemaName == null) return null;
                    Expect(TokenKind.RParen);
                    return new RefTypeExpr(schemaName, start.Merge(PrevSpan()));
                }
                case "union":
                {
                    Advance();
                    Expect(TokenKind.LParen);
                    var types = new List<TypeExpr>();
                    while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
                    {
                        SkipNewlines();
                        var t = ParseTypeExpr();
                        if (t == null) break;
                        types.Add(t);
                        SkipNewlines();
                        if (At(TokenKind.Comma)) Advance();
                        else break;
                    }
                    SkipNewlines();
                    Expect(TokenKind.RParen);
                    return new UnionTypeExpr(types, start.Merge(PrevSpan()));
                }
                default:
                    _diagnostics.Error($"unknown type: {name}", start);
                    Advance();
                    return new AnyTypeExpr(start);
            }
        }
    }
}
