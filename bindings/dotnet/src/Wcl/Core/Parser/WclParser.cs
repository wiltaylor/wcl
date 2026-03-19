using System.Collections.Generic;
using Wcl.Core.Ast;
using Wcl.Core.Tokens;

namespace Wcl.Core.Parser
{
    public partial class WclParser
    {
        private readonly List<Token> _tokens;
        private int _pos;
        private readonly DiagnosticBag _diagnostics = new DiagnosticBag();

        public WclParser(List<Token> tokens)
        {
            _tokens = tokens;
            _pos = 0;
        }

        // Token navigation
        private Token Peek() =>
            _pos < _tokens.Count ? _tokens[_pos] : _tokens[_tokens.Count - 1];

        private TokenKind PeekKind() => Peek().Kind;

        private Token Advance()
        {
            var tok = Peek();
            if (_pos < _tokens.Count) _pos++;
            return tok;
        }

        private bool At(TokenKind kind) => PeekKind() == kind;

        private Token? Expect(TokenKind kind)
        {
            if (At(kind)) return Advance();
            _diagnostics.Error($"expected {kind}, found {PeekKind()}", CurrentSpan());
            return null;
        }

        private Span CurrentSpan() => Peek().Span;
        private Span PrevSpan() => _pos > 0 ? _tokens[_pos - 1].Span : Span.Dummy();

        private void SkipNewlines()
        {
            while (PeekKind() == TokenKind.Newline || PeekKind() == TokenKind.LineComment ||
                   PeekKind() == TokenKind.BlockComment || PeekKind() == TokenKind.DocComment)
                _pos++;
        }

        private Ident? TryParseIdent()
        {
            if (PeekKind() == TokenKind.Ident)
            {
                var tok = Advance();
                return new Ident(tok.StringValue!, tok.Span);
            }
            return null;
        }

        private Ident? ExpectIdent()
        {
            if (PeekKind() == TokenKind.Ident)
            {
                var tok = Advance();
                return new Ident(tok.StringValue!, tok.Span);
            }
            _diagnostics.Error($"expected identifier, found {PeekKind()}", CurrentSpan());
            return null;
        }

        // Public API
        public static (Document, DiagnosticBag) Parse(string source, FileId fileId)
        {
            var (tokens, lexDiags) = Lexer.WclLexer.Lex(source, fileId);
            var parser = new WclParser(tokens);
            var doc = parser.ParseDocument();
            parser._diagnostics.Merge(lexDiags);
            return (doc, parser._diagnostics);
        }

        public static QueryPipeline? ParseQuery(string source, FileId fileId)
        {
            var (tokens, _) = Lexer.WclLexer.Lex(source, fileId);
            var parser = new WclParser(tokens);
            return parser.ParseQueryPipeline();
        }

        // Document parsing
        private Document ParseDocument()
        {
            var items = new List<DocItem>();
            var startSpan = CurrentSpan();

            while (!At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.Eof)) break;

                var item = ParseDocItem();
                if (item != null)
                    items.Add(item);
                else
                {
                    // Error recovery: skip token
                    Advance();
                }
            }

            var endSpan = CurrentSpan();
            return new Document(items, Trivia.Empty(), startSpan.Merge(endSpan));
        }

        private DocItem? ParseDocItem()
        {
            SkipNewlines();

            // Collect decorators
            var decorators = ParseDecorators();

            switch (PeekKind())
            {
                case TokenKind.Import:
                    return ParseImport();
                case TokenKind.Export:
                    return ParseExport();
                case TokenKind.Declare:
                    return ParseFunctionDecl();
                default:
                    var body = ParseBodyItem(decorators);
                    if (body != null) return new BodyDocItem(body);
                    return null;
            }
        }

        private DocItem ParseImport()
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'import'
            SkipNewlines();

            ImportKind kind;
            StringLit path;

            if (At(TokenKind.Lt))
            {
                // Library import: import <name.wcl>
                Advance(); // consume <
                var sb = new System.Text.StringBuilder();
                int pathStart = CurrentSpan().Start;
                while (!At(TokenKind.Gt) && !At(TokenKind.Eof))
                {
                    var tok = Advance();
                    // Reconstruct the path from tokens
                    if (tok.StringValue != null)
                        sb.Append(tok.StringValue);
                    else
                    {
                        switch (tok.Kind)
                        {
                            case TokenKind.Dot: sb.Append('.'); break;
                            case TokenKind.Slash: sb.Append('/'); break;
                            case TokenKind.Minus: sb.Append('-'); break;
                            default: sb.Append(tok.Kind.ToString().ToLower()); break;
                        }
                    }
                }
                Expect(TokenKind.Gt);
                kind = ImportKind.Library;
                var pathStr = sb.ToString();
                path = new StringLit(
                    new List<StringPart> { new LiteralPart(pathStr) },
                    new Span(startSpan.File, pathStart, PrevSpan().End));
            }
            else
            {
                kind = ImportKind.Relative;
                path = ParseStringLit()!;
                if (path == null) return new BodyDocItem(new AttributeItem(
                    new Attribute(new List<Decorator>(),
                        new Ident("_error", Span.Dummy()),
                        new NullLitExpr(Span.Dummy()),
                        Trivia.Empty(), Span.Dummy())));
            }

            var span = startSpan.Merge(PrevSpan());
            return new ImportItem(new Import(path, kind, Trivia.Empty(), span));
        }

        private DocItem? ParseExport()
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'export'
            SkipNewlines();

            if (At(TokenKind.Let))
            {
                Advance(); // consume 'let'
                SkipNewlines();
                var name = ExpectIdent();
                if (name == null) return null;
                SkipNewlines();
                Expect(TokenKind.Equals);
                SkipNewlines();
                var value = ParseExpr();
                if (value == null) return null;
                return new ExportLetItem(new ExportLet(name, value, Trivia.Empty(), startSpan.Merge(value.GetSpan())));
            }

            // Re-export
            var ident = ExpectIdent();
            if (ident == null) return null;
            return new ReExportItem(new ReExport(ident, Trivia.Empty(), startSpan.Merge(ident.Span)));
        }

        private DocItem? ParseFunctionDecl()
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'declare'
            SkipNewlines();
            var name = ExpectIdent();
            if (name == null) return null;

            Expect(TokenKind.LParen);
            var parms = new List<FunctionDeclParam>();
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                var paramName = ExpectIdent();
                if (paramName == null) break;
                Expect(TokenKind.Colon);
                SkipNewlines();
                var typeExpr = ParseTypeExpr();
                if (typeExpr == null) break;
                parms.Add(new FunctionDeclParam(paramName, typeExpr, paramName.Span.Merge(typeExpr.GetSpan())));
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RParen);

            TypeExpr? returnType = null;
            if (At(TokenKind.FatArrow) || (At(TokenKind.Minus) && _pos + 1 < _tokens.Count && _tokens[_pos + 1].Kind == TokenKind.Gt))
            {
                // -> return type
                Advance(); // consume - or =>
                if (PeekKind() == TokenKind.Gt) Advance(); // consume >
                SkipNewlines();
                returnType = ParseTypeExpr();
            }

            return new FunctionDeclItem(
                new FunctionDecl(name, parms, returnType, null, Trivia.Empty(), startSpan.Merge(PrevSpan())));
        }

        private List<Decorator> ParseDecorators()
        {
            var decorators = new List<Decorator>();
            while (At(TokenKind.At))
            {
                var startSpan = CurrentSpan();
                Advance(); // consume @
                var name = TryParseIdent() ?? ExpectIdent();
                if (name == null) break;

                var args = new List<DecoratorArg>();
                if (At(TokenKind.LParen))
                {
                    Advance(); // consume (
                    while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
                    {
                        SkipNewlines();
                        // Try named argument
                        if (PeekKind() == TokenKind.Ident)
                        {
                            int saved = _pos;
                            var argName = ExpectIdent()!;
                            SkipNewlines();
                            if (At(TokenKind.Equals))
                            {
                                Advance();
                                SkipNewlines();
                                var val = ParseExpr();
                                if (val != null)
                                    args.Add(new NamedDecoratorArg(argName, val));
                            }
                            else
                            {
                                // Positional - rewind and parse as expr
                                _pos = saved;
                                var val = ParseExpr();
                                if (val != null)
                                    args.Add(new PositionalDecoratorArg(val));
                            }
                        }
                        else
                        {
                            var val = ParseExpr();
                            if (val != null)
                                args.Add(new PositionalDecoratorArg(val));
                        }
                        SkipNewlines();
                        if (At(TokenKind.Comma)) Advance();
                        else break;
                    }
                    SkipNewlines();
                    Expect(TokenKind.RParen);
                }

                decorators.Add(new Decorator(name, args, startSpan.Merge(PrevSpan())));
                SkipNewlines();
            }
            return decorators;
        }

        private BodyItem? ParseBodyItem(List<Decorator> decorators)
        {
            SkipNewlines();

            switch (PeekKind())
            {
                case TokenKind.Let:
                    return ParseLetBinding(decorators);
                case TokenKind.Partial:
                {
                    var startSpan = CurrentSpan();
                    Advance(); // consume 'partial'
                    SkipNewlines();
                    if (At(TokenKind.Table))
                        return ParseTable(decorators, true);
                    return ParseBlock(decorators, true);
                }
                case TokenKind.Table:
                    return ParseTable(decorators, false);
                case TokenKind.Macro:
                    return ParseMacroDef(decorators);
                case TokenKind.Schema:
                    return ParseSchema(decorators);
                case TokenKind.DecoratorSchema:
                    return ParseDecoratorSchema(decorators);
                case TokenKind.Validation:
                    return ParseValidation(decorators);
                case TokenKind.For:
                    return ParseForLoop();
                case TokenKind.If:
                    return ParseConditional();
                case TokenKind.Ident:
                {
                    // Could be: attribute (name = expr), block (kind { }), or macro call (name(args))
                    return ParseIdentBodyItem(decorators);
                }
                default:
                    if (decorators.Count > 0)
                    {
                        // Decorators before something unexpected
                        _diagnostics.Error($"expected item after decorators, found {PeekKind()}", CurrentSpan());
                    }
                    return null;
            }
        }

        private BodyItem? ParseIdentBodyItem(List<Decorator> decorators)
        {
            // Lookahead to distinguish attribute, block, macro call
            int saved = _pos;
            var name = ExpectIdent()!;
            SkipNewlines();

            if (At(TokenKind.Equals))
            {
                // Attribute: name = expr
                Advance(); // consume =
                SkipNewlines();
                var value = ParseExpr();
                if (value == null) return null;
                return new AttributeItem(new Attribute(decorators, name, value, Trivia.Empty(),
                    name.Span.Merge(value.GetSpan())));
            }

            if (At(TokenKind.LBrace) || At(TokenKind.IdentifierLit) || At(TokenKind.Ident) ||
                At(TokenKind.StringLit))
            {
                // Block: kind [inline_id] ["label"...] { body }
                _pos = saved;
                return ParseBlock(decorators, false);
            }

            if (At(TokenKind.LParen))
            {
                // Macro call: name(args)
                var args = ParseMacroCallArgs();
                return new MacroCallItem(new MacroCall(name, args, Trivia.Empty(),
                    name.Span.Merge(PrevSpan())));
            }

            // Could be a block with no braces or an error
            _pos = saved;
            return ParseBlock(decorators, false);
        }

        private BodyItem? ParseLetBinding(List<Decorator> decorators)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'let'
            SkipNewlines();
            var name = ExpectIdent();
            if (name == null) return null;
            SkipNewlines();
            Expect(TokenKind.Equals);
            SkipNewlines();
            var value = ParseExpr();
            if (value == null) return null;
            return new LetBindingItem(new LetBinding(decorators, name, value, Trivia.Empty(),
                startSpan.Merge(value.GetSpan())));
        }

        internal LetBinding? ParseLetBindingDirect(List<Decorator> decorators, Trivia trivia)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'let'
            SkipNewlines();
            var name = ExpectIdent();
            if (name == null) return null;
            SkipNewlines();
            Expect(TokenKind.Equals);
            SkipNewlines();
            var value = ParseExpr();
            if (value == null) return null;
            return new LetBinding(decorators, name, value, trivia, startSpan.Merge(value.GetSpan()));
        }

        private BodyItem? ParseBlock(List<Decorator> decorators, bool partial)
        {
            var startSpan = CurrentSpan();
            var kind = ExpectIdent();
            if (kind == null) return null;
            SkipNewlines();

            // Optional inline ID
            InlineId? inlineId = null;
            if (At(TokenKind.IdentifierLit))
            {
                var tok = Advance();
                inlineId = new LiteralInlineId(new IdentifierLit(tok.StringValue!, tok.Span));
                SkipNewlines();
            }
            else if (At(TokenKind.Ident) && !At(TokenKind.LBrace))
            {
                // Check if this is an inline ID (ident not followed by =)
                int saved2 = _pos;
                var idTok = Advance();
                SkipNewlines();
                if (At(TokenKind.LBrace) || At(TokenKind.StringLit))
                {
                    // It's an inline ID
                    inlineId = new LiteralInlineId(new IdentifierLit(idTok.StringValue!, idTok.Span));
                }
                else if (At(TokenKind.Equals))
                {
                    // It's actually a nested attribute after a single-ident block kind
                    _pos = saved2;
                }
                else
                {
                    inlineId = new LiteralInlineId(new IdentifierLit(idTok.StringValue!, idTok.Span));
                }
                SkipNewlines();
            }

            // Optional labels
            var labels = new List<StringLit>();
            while (At(TokenKind.StringLit))
            {
                var sl = ParseStringLit();
                if (sl != null) labels.Add(sl);
                SkipNewlines();
            }

            // Body
            var body = new List<BodyItem>();
            if (At(TokenKind.LBrace))
            {
                Advance(); // consume {
                body = ParseBody();
                SkipNewlines();
                Expect(TokenKind.RBrace);
            }

            return new BlockItem(new Block(decorators, partial, kind, inlineId, labels, body, Trivia.Empty(),
                startSpan.Merge(PrevSpan())));
        }

        private List<BodyItem> ParseBody()
        {
            var items = new List<BodyItem>();
            while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBrace) || At(TokenKind.Eof)) break;

                var decorators = ParseDecorators();
                var item = ParseBodyItem(decorators);
                if (item != null)
                    items.Add(item);
                else
                {
                    Advance(); // error recovery
                }
            }
            return items;
        }

        private BodyItem? ParseTable(List<Decorator> decorators, bool partial)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'table'
            SkipNewlines();

            InlineId? inlineId = null;
            if (At(TokenKind.IdentifierLit))
            {
                var tok = Advance();
                inlineId = new LiteralInlineId(new IdentifierLit(tok.StringValue!, tok.Span));
                SkipNewlines();
            }
            else if (At(TokenKind.Ident))
            {
                var tok = Advance();
                inlineId = new LiteralInlineId(new IdentifierLit(tok.StringValue!, tok.Span));
                SkipNewlines();
            }

            var labels = new List<StringLit>();
            while (At(TokenKind.StringLit))
            {
                var sl = ParseStringLit();
                if (sl != null) labels.Add(sl);
                SkipNewlines();
            }

            Expect(TokenKind.LBrace);
            SkipNewlines();

            // Parse columns (name: type)
            var columns = new List<ColumnDecl>();
            while (!At(TokenKind.Pipe) && !At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.Pipe) || At(TokenKind.RBrace)) break;
                var colDecorators = ParseDecorators();
                var colName = ExpectIdent();
                if (colName == null) break;
                Expect(TokenKind.Colon);
                SkipNewlines();
                var colType = ParseTypeExpr();
                if (colType == null) break;
                columns.Add(new ColumnDecl(colDecorators, colName, colType, Trivia.Empty(),
                    colName.Span.Merge(colType.GetSpan())));
                SkipNewlines();
            }

            // Parse rows (| expr | expr | ...)
            var rows = new List<TableRow>();
            while (At(TokenKind.Pipe))
            {
                var rowStart = CurrentSpan();
                Advance(); // consume leading |
                var cells = new List<Expr>();
                while (!At(TokenKind.Newline) && !At(TokenKind.RBrace) && !At(TokenKind.Eof))
                {
                    SkipNewlines();
                    if (At(TokenKind.Pipe))
                    {
                        // Check if this is end-of-row pipe
                        int saved3 = _pos;
                        Advance();
                        SkipNewlines();
                        if (At(TokenKind.Newline) || At(TokenKind.Pipe) || At(TokenKind.RBrace) || At(TokenKind.Eof))
                        {
                            // End of row
                            break;
                        }
                        _pos = saved3;
                        Advance(); // consume separator |
                    }
                    var cell = ParseExpr();
                    if (cell != null)
                        cells.Add(cell);
                    else break;
                }
                if (cells.Count > 0)
                    rows.Add(new TableRow(cells, rowStart.Merge(PrevSpan())));
                SkipNewlines();
            }

            SkipNewlines();
            Expect(TokenKind.RBrace);

            return new TableItem(new Table(decorators, partial, inlineId, labels, columns, rows,
                Trivia.Empty(), startSpan.Merge(PrevSpan())));
        }

        private BodyItem? ParseSchema(List<Decorator> decorators)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'schema'
            SkipNewlines();
            var name = ParseStringLit();
            if (name == null) return null;
            SkipNewlines();
            Expect(TokenKind.LBrace);
            SkipNewlines();

            var fields = ParseSchemaFields();

            SkipNewlines();
            Expect(TokenKind.RBrace);

            return new SchemaItem(new Ast.Schema(decorators, name, fields, Trivia.Empty(),
                startSpan.Merge(PrevSpan())));
        }

        private List<SchemaField> ParseSchemaFields()
        {
            var fields = new List<SchemaField>();
            while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBrace)) break;

                var beforeDec = ParseDecorators();
                var fieldName = TryParseIdent();
                if (fieldName == null) break;
                Expect(TokenKind.Colon);
                SkipNewlines();
                var typeExpr = ParseTypeExpr();
                if (typeExpr == null) break;

                var afterDec = ParseDecorators();

                fields.Add(new SchemaField(beforeDec, fieldName, typeExpr, afterDec, Trivia.Empty(),
                    fieldName.Span.Merge(typeExpr.GetSpan())));
                SkipNewlines();
            }
            return fields;
        }

        private BodyItem? ParseDecoratorSchema(List<Decorator> decorators)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'decorator_schema'
            SkipNewlines();
            var name = ParseStringLit();
            if (name == null) return null;
            SkipNewlines();
            Expect(TokenKind.LBrace);
            SkipNewlines();

            // Parse target = [block, attribute, ...]
            var targets = new List<DecoratorTarget>();
            if (At(TokenKind.Ident) && Peek().StringValue == "target")
            {
                Advance(); // consume 'target'
                SkipNewlines();
                Expect(TokenKind.Equals);
                SkipNewlines();
                Expect(TokenKind.LBracket);
                SkipNewlines();
                while (!At(TokenKind.RBracket) && !At(TokenKind.Eof))
                {
                    var targetName = ExpectIdent();
                    if (targetName == null) break;
                    switch (targetName.Name)
                    {
                        case "block": targets.Add(DecoratorTarget.Block); break;
                        case "attribute": targets.Add(DecoratorTarget.Attribute); break;
                        case "table": targets.Add(DecoratorTarget.Table); break;
                        case "schema": targets.Add(DecoratorTarget.Schema); break;
                    }
                    SkipNewlines();
                    if (At(TokenKind.Comma)) Advance();
                    else break;
                    SkipNewlines();
                }
                Expect(TokenKind.RBracket);
                SkipNewlines();
            }

            var fields = ParseSchemaFields();
            SkipNewlines();
            Expect(TokenKind.RBrace);

            return new DecoratorSchemaBodyItem(new DecoratorSchema(decorators, name, targets, fields,
                Trivia.Empty(), startSpan.Merge(PrevSpan())));
        }

        private BodyItem? ParseValidation(List<Decorator> decorators)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'validation'
            SkipNewlines();
            var name = ParseStringLit();
            if (name == null) return null;
            SkipNewlines();
            Expect(TokenKind.LBrace);
            SkipNewlines();

            var lets = new List<LetBinding>();
            while (At(TokenKind.Let))
            {
                var lb = ParseLetBindingDirect(new List<Decorator>(), Trivia.Empty());
                if (lb != null) lets.Add(lb);
                SkipNewlines();
            }

            // check = expr
            Expr? check = null;
            Expr? message = null;

            while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBrace)) break;
                var attrName = ExpectIdent();
                if (attrName == null) break;
                SkipNewlines();
                Expect(TokenKind.Equals);
                SkipNewlines();
                var val = ParseExpr();
                if (val == null) break;
                if (attrName.Name == "check") check = val;
                else if (attrName.Name == "message") message = val;
                SkipNewlines();
            }

            Expect(TokenKind.RBrace);

            if (check == null) check = new BoolLitExpr(true, Span.Dummy());
            if (message == null) message = new StringLitExpr(new StringLit(
                new List<StringPart> { new LiteralPart("validation failed") }, Span.Dummy()));

            return new ValidationItem(new Validation(decorators, name, lets, check, message,
                Trivia.Empty(), startSpan.Merge(PrevSpan())));
        }

        private BodyItem? ParseMacroDef(List<Decorator> decorators)
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'macro'
            SkipNewlines();

            MacroKind macroKind;
            Ident? macroName;
            if (At(TokenKind.At))
            {
                Advance(); // consume @
                macroKind = MacroKind.Attribute;
                macroName = ExpectIdent();
            }
            else
            {
                macroKind = MacroKind.Function;
                macroName = ExpectIdent();
            }
            if (macroName == null) return null;

            // Parse params
            Expect(TokenKind.LParen);
            var parms = new List<MacroParam>();
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                var pName = ExpectIdent();
                if (pName == null) break;
                TypeExpr? typeConstraint = null;
                if (At(TokenKind.Colon))
                {
                    Advance();
                    SkipNewlines();
                    typeConstraint = ParseTypeExpr();
                }
                Expr? defaultVal = null;
                if (At(TokenKind.Equals))
                {
                    Advance();
                    SkipNewlines();
                    defaultVal = ParseExpr();
                }
                parms.Add(new MacroParam(pName, typeConstraint, defaultVal, pName.Span.Merge(PrevSpan())));
                SkipNewlines();
                if (At(TokenKind.Comma)) Advance();
                else break;
            }
            SkipNewlines();
            Expect(TokenKind.RParen);
            SkipNewlines();
            Expect(TokenKind.LBrace);
            SkipNewlines();

            MacroBody body;
            if (macroKind == MacroKind.Function)
            {
                var items = ParseBody();
                body = new FunctionMacroBody(items);
            }
            else
            {
                var directives = ParseTransformDirectives();
                body = new AttributeMacroBody(directives);
            }

            SkipNewlines();
            Expect(TokenKind.RBrace);

            return new MacroDefItem(new MacroDef(decorators, macroKind, macroName, parms, body,
                Trivia.Empty(), startSpan.Merge(PrevSpan())));
        }

        private List<TransformDirective> ParseTransformDirectives()
        {
            var directives = new List<TransformDirective>();
            while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (At(TokenKind.RBrace)) break;

                switch (PeekKind())
                {
                    case TokenKind.Inject:
                    {
                        var s = CurrentSpan();
                        Advance();
                        SkipNewlines();
                        Expect(TokenKind.LBrace);
                        var items = ParseBody();
                        SkipNewlines();
                        Expect(TokenKind.RBrace);
                        directives.Add(new InjectDirective(items, s.Merge(PrevSpan())));
                        break;
                    }
                    case TokenKind.Set:
                    {
                        var s = CurrentSpan();
                        Advance();
                        SkipNewlines();
                        Expect(TokenKind.LBrace);
                        var attrs = new List<Attribute>();
                        while (!At(TokenKind.RBrace) && !At(TokenKind.Eof))
                        {
                            SkipNewlines();
                            if (At(TokenKind.RBrace)) break;
                            var n = ExpectIdent();
                            if (n == null) break;
                            SkipNewlines();
                            Expect(TokenKind.Equals);
                            SkipNewlines();
                            var v = ParseExpr();
                            if (v == null) break;
                            attrs.Add(new Attribute(new List<Decorator>(), n, v, Trivia.Empty(), n.Span.Merge(v.GetSpan())));
                            SkipNewlines();
                        }
                        Expect(TokenKind.RBrace);
                        directives.Add(new SetDirective(attrs, s.Merge(PrevSpan())));
                        break;
                    }
                    case TokenKind.Remove:
                    {
                        var s = CurrentSpan();
                        Advance();
                        SkipNewlines();
                        Expect(TokenKind.LBracket);
                        var names = new List<Ident>();
                        while (!At(TokenKind.RBracket) && !At(TokenKind.Eof))
                        {
                            SkipNewlines();
                            var n = ExpectIdent();
                            if (n == null) break;
                            names.Add(n);
                            SkipNewlines();
                            if (At(TokenKind.Comma)) Advance();
                            else break;
                        }
                        Expect(TokenKind.RBracket);
                        directives.Add(new RemoveDirective(names, s.Merge(PrevSpan())));
                        break;
                    }
                    case TokenKind.When:
                    {
                        var s = CurrentSpan();
                        Advance();
                        SkipNewlines();
                        var cond = ParseExpr();
                        if (cond == null) break;
                        SkipNewlines();
                        Expect(TokenKind.LBrace);
                        var inner = ParseTransformDirectives();
                        SkipNewlines();
                        Expect(TokenKind.RBrace);
                        directives.Add(new WhenDirective(cond, inner, s.Merge(PrevSpan())));
                        break;
                    }
                    default:
                        Advance(); // skip unknown
                        break;
                }
                SkipNewlines();
            }
            return directives;
        }

        private List<MacroCallArg> ParseMacroCallArgs()
        {
            var args = new List<MacroCallArg>();
            Expect(TokenKind.LParen);
            while (!At(TokenKind.RParen) && !At(TokenKind.Eof))
            {
                SkipNewlines();
                if (PeekKind() == TokenKind.Ident)
                {
                    int saved = _pos;
                    var argName = ExpectIdent()!;
                    SkipNewlines();
                    if (At(TokenKind.Equals))
                    {
                        Advance();
                        SkipNewlines();
                        var val = ParseExpr();
                        if (val != null) args.Add(new NamedMacroArg(argName, val));
                    }
                    else
                    {
                        _pos = saved;
                        var val = ParseExpr();
                        if (val != null) args.Add(new PositionalMacroArg(val));
                    }
                }
                else
                {
                    var val = ParseExpr();
                    if (val != null) args.Add(new PositionalMacroArg(val));
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

        private BodyItem? ParseForLoop()
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'for'
            SkipNewlines();
            var iterator = ExpectIdent();
            if (iterator == null) return null;

            Ident? index = null;
            SkipNewlines();
            if (At(TokenKind.Comma))
            {
                Advance();
                SkipNewlines();
                index = ExpectIdent();
            }

            SkipNewlines();
            Expect(TokenKind.In);
            SkipNewlines();
            var iterable = ParseExpr();
            if (iterable == null) return null;
            SkipNewlines();
            Expect(TokenKind.LBrace);
            var body = ParseBody();
            SkipNewlines();
            Expect(TokenKind.RBrace);

            return new ForLoopItem(new ForLoop(iterator, index, iterable, body, Trivia.Empty(),
                startSpan.Merge(PrevSpan())));
        }

        private BodyItem? ParseConditional()
        {
            var cond = ParseConditionalInner();
            if (cond == null) return null;
            return new ConditionalItem(cond);
        }

        private Conditional? ParseConditionalInner()
        {
            var startSpan = CurrentSpan();
            Advance(); // consume 'if'
            SkipNewlines();
            var condition = ParseExpr();
            if (condition == null) return null;
            SkipNewlines();
            Expect(TokenKind.LBrace);
            var thenBody = ParseBody();
            SkipNewlines();
            Expect(TokenKind.RBrace);

            ElseBranch? elseBranch = null;
            SkipNewlines();
            if (At(TokenKind.Else))
            {
                Advance();
                SkipNewlines();
                if (At(TokenKind.If))
                {
                    var elseIf = ParseConditionalInner();
                    if (elseIf != null) elseBranch = new ElseIfBranch(elseIf);
                }
                else
                {
                    var elseStart = CurrentSpan();
                    Expect(TokenKind.LBrace);
                    var elseBody = ParseBody();
                    SkipNewlines();
                    Expect(TokenKind.RBrace);
                    elseBranch = new ElseBlock(elseBody, Trivia.Empty(), elseStart.Merge(PrevSpan()));
                }
            }

            return new Conditional(condition, thenBody, elseBranch, Trivia.Empty(),
                startSpan.Merge(PrevSpan()));
        }

        // String literal parsing (handles interpolation)
        internal StringLit? ParseStringLit()
        {
            if (PeekKind() == TokenKind.Heredoc)
            {
                var tok = Advance();
                return new StringLit(
                    new List<StringPart> { new LiteralPart(tok.StringValue!) },
                    tok.Span);
            }

            if (!At(TokenKind.StringLit))
            {
                _diagnostics.Error($"expected string literal, found {PeekKind()}", CurrentSpan());
                return null;
            }

            var strTok = Advance();
            var raw = strTok.StringValue!;

            // Check for interpolation markers ${
            if (!raw.Contains("${"))
            {
                return new StringLit(
                    new List<StringPart> { new LiteralPart(raw) },
                    strTok.Span);
            }

            // Parse interpolated string
            var parts = new List<StringPart>();
            int i = 0;
            while (i < raw.Length)
            {
                int interpIdx = raw.IndexOf("${", i);
                if (interpIdx < 0)
                {
                    parts.Add(new LiteralPart(raw.Substring(i)));
                    break;
                }
                if (interpIdx > i)
                    parts.Add(new LiteralPart(raw.Substring(i, interpIdx - i)));

                // Find matching }
                int depth = 1;
                int j = interpIdx + 2;
                while (j < raw.Length && depth > 0)
                {
                    if (raw[j] == '{') depth++;
                    else if (raw[j] == '}') depth--;
                    if (depth > 0) j++;
                }

                string exprStr = raw.Substring(interpIdx + 2, j - interpIdx - 2);
                // Sub-parse the expression
                var (subTokens, _) = Lexer.WclLexer.Lex(exprStr, strTok.Span.File);
                var subParser = new WclParser(subTokens);
                var expr = subParser.ParseExpr();
                if (expr != null)
                    parts.Add(new InterpolationPart(expr));

                i = j + 1; // skip past closing }
            }

            return new StringLit(parts, strTok.Span);
        }
    }
}
