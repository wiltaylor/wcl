using System.Collections.Generic;
using Wcl.Core.Ast;
using Wcl.Core.Tokens;

namespace Wcl.Core.Parser
{
    public partial class WclParser
    {
        internal Expr? ParseExpr() => ParseTernary();

        private Expr? ParseTernary()
        {
            var cond = ParseBinary(2);
            if (cond == null) return null;

            if (PeekKind() == TokenKind.Question)
            {
                Advance(); // consume ?
                var thenExpr = ParseExpr();
                if (thenExpr == null) return null;
                Expect(TokenKind.Colon);
                var elseExpr = ParseExpr();
                if (elseExpr == null) return null;
                return new TernaryExpr(cond, thenExpr, elseExpr, cond.GetSpan().Merge(elseExpr.GetSpan()));
            }
            return cond;
        }

        private Expr? ParseBinary(int minPrec)
        {
            var lhs = ParseUnary();
            if (lhs == null) return null;

            while (true)
            {
                var op = TokenToBinOp();
                if (op == null || op.Value.Precedence() < minPrec) break;

                Advance(); // consume operator
                int nextPrec = op.Value.Precedence() + 1; // left-associative
                var rhs = ParseBinary(nextPrec);
                if (rhs == null) return null;
                lhs = new BinaryOpExpr(lhs, op.Value, rhs, lhs.GetSpan().Merge(rhs.GetSpan()));
            }
            return lhs;
        }

        private BinOp? TokenToBinOp() => PeekKind() switch
        {
            TokenKind.Or => BinOp.Or,
            TokenKind.And => BinOp.And,
            TokenKind.EqEq => BinOp.Eq,
            TokenKind.Neq => BinOp.Neq,
            TokenKind.Lt => BinOp.Lt,
            TokenKind.Gt => BinOp.Gt,
            TokenKind.Lte => BinOp.Lte,
            TokenKind.Gte => BinOp.Gte,
            TokenKind.Match => BinOp.Match,
            TokenKind.Plus => BinOp.Add,
            TokenKind.Minus => BinOp.Sub,
            TokenKind.Star => BinOp.Mul,
            TokenKind.Slash => BinOp.Div,
            TokenKind.Percent => BinOp.Mod,
            _ => null,
        };

        private Expr? ParseUnary()
        {
            switch (PeekKind())
            {
                case TokenKind.Not:
                {
                    var start = CurrentSpan();
                    Advance();
                    var expr = ParseUnary();
                    if (expr == null) return null;
                    return new UnaryOpExpr(UnaryOp.Not, expr, start.Merge(expr.GetSpan()));
                }
                case TokenKind.Minus:
                {
                    var start = CurrentSpan();
                    Advance();
                    var expr = ParseUnary();
                    if (expr == null) return null;
                    return new UnaryOpExpr(UnaryOp.Neg, expr, start.Merge(expr.GetSpan()));
                }
                default:
                {
                    var primary = ParsePrimary();
                    if (primary == null) return null;
                    return ParsePostfix(primary);
                }
            }
        }

        private Expr ParsePostfix(Expr lhs)
        {
            while (true)
            {
                switch (PeekKind())
                {
                    case TokenKind.Dot:
                    {
                        Advance(); // consume .
                        var ident = TryParseIdent();
                        if (ident == null)
                        {
                            _diagnostics.Error("expected identifier after '.'", CurrentSpan());
                            return lhs;
                        }
                        lhs = new MemberAccessExpr(lhs, ident, lhs.GetSpan().Merge(ident.Span));
                        break;
                    }
                    case TokenKind.LBracket:
                    {
                        Advance(); // consume [
                        var index = ParseExpr();
                        if (index == null) return lhs;
                        Expect(TokenKind.RBracket);
                        lhs = new IndexAccessExpr(lhs, index, lhs.GetSpan().Merge(PrevSpan()));
                        break;
                    }
                    case TokenKind.LParen:
                    {
                        var args = ParseCallArgs();
                        lhs = new FnCallExpr(lhs, args, lhs.GetSpan().Merge(PrevSpan()));
                        break;
                    }
                    default:
                        return lhs;
                }
            }
        }

        private Expr? ParsePrimary()
        {
            SkipNewlines();

            switch (PeekKind())
            {
                case TokenKind.IntLit:
                {
                    var tok = Advance();
                    return new IntLitExpr(tok.IntValue, tok.Span);
                }
                case TokenKind.FloatLit:
                {
                    var tok = Advance();
                    return new FloatLitExpr(tok.DoubleValue, tok.Span);
                }
                case TokenKind.BoolLit:
                {
                    var tok = Advance();
                    return new BoolLitExpr(tok.BoolValue, tok.Span);
                }
                case TokenKind.NullLit:
                {
                    var tok = Advance();
                    return new NullLitExpr(tok.Span);
                }
                case TokenKind.StringLit:
                case TokenKind.Heredoc:
                {
                    var s = ParseStringLit();
                    if (s == null) return null;
                    return new StringLitExpr(s);
                }
                case TokenKind.IdentifierLit:
                {
                    var tok = Advance();
                    return new IdentifierLitExpr(new IdentifierLit(tok.StringValue!, tok.Span));
                }
                case TokenKind.Ident:
                {
                    var name = Peek().StringValue!;
                    if (name == "import_raw") return ParseImportRawExpr();
                    if (name == "import_table") return ParseImportTableExpr();
                    if (name == "set") return ParseSetExpr();
                    return ParseIdentOrLambda();
                }
                case TokenKind.Query:
                    return ParseQueryExpr();
                case TokenKind.Ref:
                    return ParseRefExpr();
                case TokenKind.SelfKw:
                {
                    var span = CurrentSpan();
                    Advance();
                    return new IdentExpr(new Ident("self", span));
                }
                case TokenKind.LBracket:
                    return ParseListLiteral();
                case TokenKind.LBrace:
                    return ParseMapOrBlockExpr();
                case TokenKind.LParen:
                    return ParseParenOrLambda();
                default:
                    _diagnostics.Error($"expected expression, found {PeekKind()}", CurrentSpan());
                    return null;
            }
        }

        private Expr? ParseIdentOrLambda()
        {
            var ident = TryParseIdent();
            if (ident == null) return null;

            if (PeekKind() == TokenKind.FatArrow)
            {
                Advance(); // consume =>
                var body = ParseExpr();
                if (body == null) return null;
                return new LambdaExpr(new List<Ident> { ident }, body, ident.Span.Merge(body.GetSpan()));
            }
            return new IdentExpr(ident);
        }

        private Expr? ParseSetExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume 'set'
            if (!At(TokenKind.LParen)) return new IdentExpr(new Ident("set", start));

            Advance(); // consume (
            var items = new List<Expr>();
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                var expr = ParseExpr();
                if (expr != null) items.Add(expr);
                else break;
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RParen);
            return new SetExpr(items, start.Merge(PrevSpan()));
        }

        private Expr? ParseParenOrLambda()
        {
            var start = CurrentSpan();
            if (IsLambdaParams())
                return ParseLambda();

            Advance(); // consume (
            SkipNewlines();
            var inner = ParseExpr();
            if (inner == null) return null;
            SkipNewlines();
            Expect(TokenKind.RParen);
            return new ParenExpr(inner, start.Merge(PrevSpan()));
        }

        private bool IsLambdaParams()
        {
            if (PeekKind() != TokenKind.LParen) return false;
            int i = _pos + 1;
            while (true)
            {
                while (i < _tokens.Count && _tokens[i].Kind == TokenKind.Newline) i++;
                if (i >= _tokens.Count) return false;
                if (_tokens[i].Kind == TokenKind.Ident)
                {
                    i++;
                    while (i < _tokens.Count && _tokens[i].Kind == TokenKind.Newline) i++;
                    if (i >= _tokens.Count) return false;
                    if (_tokens[i].Kind == TokenKind.Comma) { i++; continue; }
                    if (_tokens[i].Kind == TokenKind.RParen)
                    {
                        i++;
                        while (i < _tokens.Count && _tokens[i].Kind == TokenKind.Newline) i++;
                        return i < _tokens.Count && _tokens[i].Kind == TokenKind.FatArrow;
                    }
                    return false;
                }
                if (_tokens[i].Kind == TokenKind.RParen)
                {
                    i++;
                    while (i < _tokens.Count && _tokens[i].Kind == TokenKind.Newline) i++;
                    return i < _tokens.Count && _tokens[i].Kind == TokenKind.FatArrow;
                }
                return false;
            }
        }

        private Expr? ParseLambda()
        {
            var start = CurrentSpan();
            Advance(); // consume (
            var parms = new List<Ident>();
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RParen)) break;
                var ident = ExpectIdent();
                if (ident == null) return null;
                parms.Add(ident);
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RParen);
            Expect(TokenKind.FatArrow);
            var body = ParseExpr();
            if (body == null) return null;
            return new LambdaExpr(parms, body, start.Merge(body.GetSpan()));
        }

        private Expr? ParseListLiteral()
        {
            var start = CurrentSpan();
            Advance(); // consume [
            var items = new List<Expr>();
            while (!At(TokenKind.RBracket) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBracket)) break;
                var expr = ParseExpr();
                if (expr != null) items.Add(expr);
                else break;
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RBracket);
            return new ListExpr(items, start.Merge(PrevSpan()));
        }

        private Expr? ParseMapOrBlockExpr()
        {
            // Disambiguate
            int i = _pos + 1;
            while (i < _tokens.Count && _tokens[i].Kind == TokenKind.Newline) i++;
            if (i < _tokens.Count)
            {
                if (_tokens[i].Kind == TokenKind.Let) return ParseBlockExpr();
                if (_tokens[i].Kind == TokenKind.RBrace) return ParseMapLiteral();
                if (_tokens[i].Kind == TokenKind.Ident || _tokens[i].Kind == TokenKind.StringLit)
                {
                    int j = i + 1;
                    while (j < _tokens.Count && _tokens[j].Kind == TokenKind.Newline) j++;
                    if (j < _tokens.Count && _tokens[j].Kind == TokenKind.Equals)
                        return ParseMapLiteral();
                }
            }
            return ParseMapLiteral();
        }

        private Expr? ParseMapLiteral()
        {
            var start = CurrentSpan();
            Advance(); // consume {
            var entries = new List<(MapKey, Expr)>();
            while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBrace)) break;

                MapKey key;
                if (PeekKind() == TokenKind.Ident)
                {
                    var tok = Advance();
                    key = new IdentMapKey(new Ident(tok.StringValue!, tok.Span));
                }
                else if (PeekKind() == TokenKind.StringLit)
                {
                    var s = ParseStringLit();
                    if (s == null) break;
                    key = new StringMapKey(s);
                }
                else
                {
                    _diagnostics.Error("expected map key", CurrentSpan());
                    break;
                }
                SkipNewlines();
                Expect(TokenKind.Equals);
                SkipNewlines();
                var value = ParseExpr();
                if (value == null) break;
                entries.Add((key, value));
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
            }
            SkipNewlines();
            Expect(TokenKind.RBrace);
            return new MapExpr(entries, start.Merge(PrevSpan()));
        }

        private Expr? ParseBlockExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume {
            var lets = new List<LetBinding>();
            while (At(TokenKind.Let) || PeekKind() == TokenKind.Newline)
            {
                SkipNewlines();
                if (!At(TokenKind.Let)) break;
                var lb = ParseLetBindingDirect(new List<Decorator>(), Trivia.Empty());
                if (lb != null) lets.Add(lb);
            }
            SkipNewlines();
            var finalExpr = ParseExpr();
            if (finalExpr == null) return null;
            SkipNewlines();
            Expect(TokenKind.RBrace);
            return new BlockExprNode(lets, finalExpr, start.Merge(PrevSpan()));
        }

        private Expr? ParseQueryExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume 'query'
            Expect(TokenKind.LParen);
            var pipeline = ParseQueryPipeline();
            if (pipeline == null) return null;
            Expect(TokenKind.RParen);
            return new QueryExpr(pipeline, start.Merge(PrevSpan()));
        }

        internal QueryPipeline? ParseQueryPipeline()
        {
            SkipNewlines();
            var start = CurrentSpan();
            var selector = ParseQuerySelector();
            if (selector == null) return null;

            var filters = new List<QueryFilter>();
            while (true)
            {
                SkipNewlines();
                if (PeekKind() != TokenKind.Pipe) break;
                Advance(); // consume |
                SkipNewlines();
                var filter = ParseQueryFilter();
                if (filter != null) filters.Add(filter);
                else break;
            }

            return new QueryPipeline(selector, filters, start.Merge(PrevSpan()));
        }

        private QuerySelector? ParseQuerySelector()
        {
            SkipNewlines();
            switch (PeekKind())
            {
                case TokenKind.Dot:
                {
                    Advance();
                    if (At(TokenKind.Dot))
                    {
                        Advance();
                        var kind = ExpectIdent();
                        if (kind == null) return null;
                        if (At(TokenKind.Hash))
                        {
                            Advance();
                            return ParseIdAfterHash(kind, (k, id) => new RecursiveIdSelector(k, id));
                        }
                        return new RecursiveSelector(kind);
                    }
                    return new RootSelector();
                }
                case TokenKind.DotDot:
                {
                    Advance();
                    var kind = ExpectIdent();
                    if (kind == null) return null;
                    if (At(TokenKind.Hash))
                    {
                        Advance();
                        return ParseIdAfterHash(kind, (k, id) => new RecursiveIdSelector(k, id));
                    }
                    return new RecursiveSelector(kind);
                }
                case TokenKind.Star:
                    Advance();
                    return new WildcardSelector();
                case TokenKind.Table:
                {
                    var span = CurrentSpan();
                    Advance();
                    if (At(TokenKind.Hash))
                    {
                        Advance();
                        return ParseTableIdAfterHash();
                    }
                    if (At(TokenKind.Dot))
                    {
                        Advance();
                        if (At(TokenKind.StringLit))
                        {
                            var s = ParseStringLit()!;
                            return new TableLabelSelector(s);
                        }
                        // Path
                        var tableIdent = new Ident("table", span);
                        var segments = new List<PathSegment> { new IdentPathSegment(tableIdent) };
                        ParsePathContinuation(segments);
                        return new PathSelector(segments);
                    }
                    return new KindSelector(new Ident("table", span));
                }
                case TokenKind.Ident:
                {
                    var ident = ExpectIdent()!;
                    if (At(TokenKind.Hash))
                    {
                        Advance();
                        return ParseIdAfterHash(ident, (k, id) => new KindIdSelector(k, id));
                    }
                    if (At(TokenKind.Dot))
                    {
                        var segments = new List<PathSegment> { new IdentPathSegment(ident) };
                        while (At(TokenKind.Dot))
                        {
                            Advance();
                            if (At(TokenKind.Ident))
                            {
                                var seg = ExpectIdent()!;
                                segments.Add(new IdentPathSegment(seg));
                            }
                            else if (At(TokenKind.StringLit))
                            {
                                var s = ParseStringLit()!;
                                segments.Add(new StringLabelPathSegment(s));
                            }
                            else break;
                        }
                        return new PathSelector(segments);
                    }
                    return new KindSelector(ident);
                }
                case TokenKind.StringLit:
                {
                    var s = ParseStringLit()!;
                    return new TableLabelSelector(s);
                }
                default:
                    _diagnostics.Error($"expected query selector, found {PeekKind()}", CurrentSpan());
                    return null;
            }
        }

        private T ParseIdAfterHash<T>(Ident kind, System.Func<Ident, IdentifierLit, T> factory)
        {
            if (At(TokenKind.IdentifierLit))
            {
                var tok = Advance();
                return factory(kind, new IdentifierLit(tok.StringValue!, tok.Span));
            }
            if (At(TokenKind.Ident))
            {
                var tok = Advance();
                return factory(kind, new IdentifierLit(tok.StringValue!, tok.Span));
            }
            _diagnostics.Error("expected identifier after '#'", CurrentSpan());
            return factory(kind, new IdentifierLit("", Span.Dummy()));
        }

        private QuerySelector ParseTableIdAfterHash()
        {
            if (At(TokenKind.IdentifierLit))
            {
                var tok = Advance();
                return new TableIdSelector(new IdentifierLit(tok.StringValue!, tok.Span));
            }
            if (At(TokenKind.Ident))
            {
                var tok = Advance();
                return new TableIdSelector(new IdentifierLit(tok.StringValue!, tok.Span));
            }
            _diagnostics.Error("expected identifier after '#'", CurrentSpan());
            return new TableIdSelector(new IdentifierLit("", Span.Dummy()));
        }

        private void ParsePathContinuation(List<PathSegment> segments)
        {
            if (At(TokenKind.Ident))
            {
                var seg = ExpectIdent()!;
                segments.Add(new IdentPathSegment(seg));
            }
            while (At(TokenKind.Dot))
            {
                Advance();
                if (At(TokenKind.Ident))
                {
                    var seg = ExpectIdent()!;
                    segments.Add(new IdentPathSegment(seg));
                }
                else if (At(TokenKind.StringLit))
                {
                    var s = ParseStringLit()!;
                    segments.Add(new StringLabelPathSegment(s));
                }
                else break;
            }
        }

        private QueryFilter? ParseQueryFilter()
        {
            SkipNewlines();
            switch (PeekKind())
            {
                case TokenKind.Dot:
                {
                    Advance();
                    var attr = ExpectIdent();
                    if (attr == null) return null;
                    var op = TokenToBinOp();
                    if (op != null)
                    {
                        Advance();
                        var expr = ParseExpr();
                        if (expr == null) return null;
                        return new AttrComparisonFilter(attr, op.Value, expr);
                    }
                    return new ProjectionFilter(attr);
                }
                case TokenKind.Ident when Peek().StringValue == "has":
                {
                    Advance();
                    Expect(TokenKind.LParen);
                    SkipNewlines();
                    if (At(TokenKind.At))
                    {
                        Advance();
                        var name = ExpectIdent();
                        if (name == null) return null;
                        Expect(TokenKind.RParen);
                        return new HasDecoratorFilter(name);
                    }
                    if (At(TokenKind.Dot))
                    {
                        Advance();
                        var attr = ExpectIdent();
                        if (attr == null) return null;
                        Expect(TokenKind.RParen);
                        return new HasAttrFilter(attr);
                    }
                    _diagnostics.Error("expected '.attr' or '@decorator' inside has()", CurrentSpan());
                    return null;
                }
                case TokenKind.At:
                {
                    Advance();
                    var decName = ExpectIdent();
                    if (decName == null) return null;
                    Expect(TokenKind.Dot);
                    var paramName = ExpectIdent();
                    if (paramName == null) return null;
                    var op = TokenToBinOp();
                    if (op == null)
                    {
                        _diagnostics.Error("expected comparison operator in decorator filter", CurrentSpan());
                        return null;
                    }
                    Advance();
                    var expr = ParseExpr();
                    if (expr == null) return null;
                    return new DecoratorArgFilterNode(decName, paramName, op.Value, expr);
                }
                default:
                    _diagnostics.Error($"expected query filter, found {PeekKind()}", CurrentSpan());
                    return null;
            }
        }

        private Expr? ParseRefExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume 'ref'
            Expect(TokenKind.LParen);
            IdentifierLit idLit;
            if (At(TokenKind.IdentifierLit))
            {
                var tok = Advance();
                idLit = new IdentifierLit(tok.StringValue!, tok.Span);
            }
            else if (At(TokenKind.Ident))
            {
                var tok = Advance();
                idLit = new IdentifierLit(tok.StringValue!, tok.Span);
            }
            else
            {
                _diagnostics.Error("expected identifier in ref()", CurrentSpan());
                return null;
            }
            Expect(TokenKind.RParen);
            return new RefExpr(idLit, start.Merge(PrevSpan()));
        }

        private Expr? ParseImportRawExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume 'import_raw'
            Expect(TokenKind.LParen);
            var path = ParseStringLit();
            if (path == null) return null;
            Expect(TokenKind.RParen);
            return new ImportRawExpr(path, start.Merge(PrevSpan()));
        }

        private Expr? ParseImportTableExpr()
        {
            var start = CurrentSpan();
            Advance(); // consume 'import_table'
            Expect(TokenKind.LParen);
            var path = ParseStringLit();
            if (path == null) return null;
            StringLit? separator = null;
            if (At(TokenKind.Comma))
            {
                Advance();
                separator = ParseStringLit();
            }
            Expect(TokenKind.RParen);
            return new ImportTableExpr(path, separator, start.Merge(PrevSpan()));
        }

        private List<CallArg> ParseCallArgs()
        {
            var args = new List<CallArg>();
            Expect(TokenKind.LParen);
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                // Try named argument
                if (PeekKind() == TokenKind.Ident)
                {
                    int saved = _pos;
                    var argName = ExpectIdent()!;
                    SkipNewlines();
                    // Look for =
                    if (At(TokenKind.Equals))
                    {
                        Advance();
                        SkipNewlines();
                        var val = ParseExpr();
                        if (val != null) args.Add(new NamedCallArg(argName, val));
                    }
                    else
                    {
                        _pos = saved;
                        var val = ParseExpr();
                        if (val != null) args.Add(new PositionalCallArg(val));
                    }
                }
                else
                {
                    var val = ParseExpr();
                    if (val != null) args.Add(new PositionalCallArg(val));
                    else break;
                }
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RParen);
            return args;
        }
    }
}
