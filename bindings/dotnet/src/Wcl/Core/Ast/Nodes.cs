using System.Collections.Generic;

namespace Wcl.Core.Ast
{
    // Identifiers
    public class Ident
    {
        public string Name { get; set; }
        public Span Span { get; set; }
        public Ident(string name, Span span) { Name = name; Span = span; }
    }

    public class IdentifierLit
    {
        public string Value { get; set; }
        public Span Span { get; set; }
        public IdentifierLit(string value, Span span) { Value = value; Span = span; }
    }

    // String literal
    public class StringLit
    {
        public List<StringPart> Parts { get; set; }
        public Span Span { get; set; }
        public StringLit(List<StringPart> parts, Span span) { Parts = parts; Span = span; }
    }

    public abstract class StringPart { }
    public sealed class LiteralPart : StringPart
    {
        public string Value { get; }
        public LiteralPart(string value) => Value = value;
    }
    public sealed class InterpolationPart : StringPart
    {
        public Expr Expr { get; }
        public InterpolationPart(Expr expr) => Expr = expr;
    }

    // Import
    public class Import
    {
        public StringLit Path { get; set; }
        public ImportKind Kind { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Import(StringLit path, ImportKind kind, Trivia trivia, Span span)
        { Path = path; Kind = kind; Trivia = trivia; Span = span; }
    }

    // Export let
    public class ExportLet
    {
        public Ident Name { get; set; }
        public Expr Value { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public ExportLet(Ident name, Expr value, Trivia trivia, Span span)
        { Name = name; Value = value; Trivia = trivia; Span = span; }
    }

    // Re-export
    public class ReExport
    {
        public Ident Name { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public ReExport(Ident name, Trivia trivia, Span span)
        { Name = name; Trivia = trivia; Span = span; }
    }

    // Function declaration
    public class FunctionDecl
    {
        public Ident Name { get; set; }
        public List<FunctionDeclParam> Params { get; set; }
        public TypeExpr? ReturnType { get; set; }
        public string? Doc { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public FunctionDecl(Ident name, List<FunctionDeclParam> parms, TypeExpr? returnType,
                            string? doc, Trivia trivia, Span span)
        { Name = name; Params = parms; ReturnType = returnType; Doc = doc; Trivia = trivia; Span = span; }
    }

    public class FunctionDeclParam
    {
        public Ident Name { get; set; }
        public TypeExpr TypeExpr { get; set; }
        public Span Span { get; set; }
        public FunctionDeclParam(Ident name, TypeExpr typeExpr, Span span)
        { Name = name; TypeExpr = typeExpr; Span = span; }
    }

    // Attribute
    public class Attribute
    {
        public List<Decorator> Decorators { get; set; }
        public Ident Name { get; set; }
        public Expr Value { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Attribute(List<Decorator> decorators, Ident name, Expr value, Trivia trivia, Span span)
        { Decorators = decorators; Name = name; Value = value; Trivia = trivia; Span = span; }
    }

    // Block
    public class Block
    {
        public List<Decorator> Decorators { get; set; }
        public bool Partial { get; set; }
        public Ident Kind { get; set; }
        public InlineId? InlineId { get; set; }
        public List<StringLit> Labels { get; set; }
        public List<BodyItem> Body { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Block(List<Decorator> decorators, bool partial, Ident kind, InlineId? inlineId,
                     List<StringLit> labels, List<BodyItem> body, Trivia trivia, Span span)
        {
            Decorators = decorators; Partial = partial; Kind = kind; InlineId = inlineId;
            Labels = labels; Body = body; Trivia = trivia; Span = span;
        }
    }

    // InlineId
    public abstract class InlineId { }
    public sealed class LiteralInlineId : InlineId
    {
        public IdentifierLit Lit { get; }
        public LiteralInlineId(IdentifierLit lit) => Lit = lit;
    }
    public sealed class InterpolatedInlineId : InlineId
    {
        public List<StringPart> Parts { get; }
        public InterpolatedInlineId(List<StringPart> parts) => Parts = parts;
    }

    // Let binding
    public class LetBinding
    {
        public List<Decorator> Decorators { get; set; }
        public Ident Name { get; set; }
        public Expr Value { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public LetBinding(List<Decorator> decorators, Ident name, Expr value, Trivia trivia, Span span)
        { Decorators = decorators; Name = name; Value = value; Trivia = trivia; Span = span; }
    }

    // Decorator
    public class Decorator
    {
        public Ident Name { get; set; }
        public List<DecoratorArg> Args { get; set; }
        public Span Span { get; set; }
        public Decorator(Ident name, List<DecoratorArg> args, Span span)
        { Name = name; Args = args; Span = span; }
    }

    public abstract class DecoratorArg { }
    public sealed class PositionalDecoratorArg : DecoratorArg
    {
        public Expr Value { get; }
        public PositionalDecoratorArg(Expr value) => Value = value;
    }
    public sealed class NamedDecoratorArg : DecoratorArg
    {
        public Ident Name { get; }
        public Expr Value { get; }
        public NamedDecoratorArg(Ident name, Expr value) { Name = name; Value = value; }
    }

    // Table
    public class Table
    {
        public List<Decorator> Decorators { get; set; }
        public bool Partial { get; set; }
        public InlineId? InlineId { get; set; }
        public List<StringLit> Labels { get; set; }
        public List<ColumnDecl> Columns { get; set; }
        public List<TableRow> Rows { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Table(List<Decorator> decorators, bool partial, InlineId? inlineId,
                     List<StringLit> labels, List<ColumnDecl> columns, List<TableRow> rows,
                     Trivia trivia, Span span)
        {
            Decorators = decorators; Partial = partial; InlineId = inlineId;
            Labels = labels; Columns = columns; Rows = rows; Trivia = trivia; Span = span;
        }
    }

    public class ColumnDecl
    {
        public List<Decorator> Decorators { get; set; }
        public Ident Name { get; set; }
        public TypeExpr TypeExpr { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public ColumnDecl(List<Decorator> decorators, Ident name, TypeExpr typeExpr, Trivia trivia, Span span)
        { Decorators = decorators; Name = name; TypeExpr = typeExpr; Trivia = trivia; Span = span; }
    }

    public class TableRow
    {
        public List<Expr> Cells { get; set; }
        public Span Span { get; set; }
        public TableRow(List<Expr> cells, Span span) { Cells = cells; Span = span; }
    }

    // Schema
    public class Schema
    {
        public List<Decorator> Decorators { get; set; }
        public StringLit Name { get; set; }
        public List<SchemaField> Fields { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Schema(List<Decorator> decorators, StringLit name, List<SchemaField> fields,
                      Trivia trivia, Span span)
        { Decorators = decorators; Name = name; Fields = fields; Trivia = trivia; Span = span; }
    }

    public class SchemaField
    {
        public List<Decorator> DecoratorsBefore { get; set; }
        public Ident Name { get; set; }
        public TypeExpr TypeExpr { get; set; }
        public List<Decorator> DecoratorsAfter { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public SchemaField(List<Decorator> decoratorsBefore, Ident name, TypeExpr typeExpr,
                           List<Decorator> decoratorsAfter, Trivia trivia, Span span)
        {
            DecoratorsBefore = decoratorsBefore; Name = name; TypeExpr = typeExpr;
            DecoratorsAfter = decoratorsAfter; Trivia = trivia; Span = span;
        }
    }

    // Decorator schema
    public class DecoratorSchema
    {
        public List<Decorator> Decorators { get; set; }
        public StringLit Name { get; set; }
        public List<DecoratorTarget> Target { get; set; }
        public List<SchemaField> Fields { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public DecoratorSchema(List<Decorator> decorators, StringLit name, List<DecoratorTarget> target,
                                List<SchemaField> fields, Trivia trivia, Span span)
        {
            Decorators = decorators; Name = name; Target = target;
            Fields = fields; Trivia = trivia; Span = span;
        }
    }

    // Macros
    public class MacroDef
    {
        public List<Decorator> Decorators { get; set; }
        public MacroKind Kind { get; set; }
        public Ident Name { get; set; }
        public List<MacroParam> Params { get; set; }
        public MacroBody Body { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public MacroDef(List<Decorator> decorators, MacroKind kind, Ident name,
                        List<MacroParam> parms, MacroBody body, Trivia trivia, Span span)
        {
            Decorators = decorators; Kind = kind; Name = name;
            Params = parms; Body = body; Trivia = trivia; Span = span;
        }
    }

    public class MacroParam
    {
        public Ident Name { get; set; }
        public TypeExpr? TypeConstraint { get; set; }
        public Expr? Default { get; set; }
        public Span Span { get; set; }
        public MacroParam(Ident name, TypeExpr? typeConstraint, Expr? defaultVal, Span span)
        { Name = name; TypeConstraint = typeConstraint; Default = defaultVal; Span = span; }
    }

    public abstract class MacroBody { }
    public sealed class FunctionMacroBody : MacroBody
    {
        public List<BodyItem> Items { get; }
        public FunctionMacroBody(List<BodyItem> items) => Items = items;
    }
    public sealed class AttributeMacroBody : MacroBody
    {
        public List<TransformDirective> Directives { get; }
        public AttributeMacroBody(List<TransformDirective> directives) => Directives = directives;
    }

    // Transform directives
    public abstract class TransformDirective { }
    public sealed class InjectDirective : TransformDirective
    {
        public List<BodyItem> Body { get; }
        public Span Span { get; }
        public InjectDirective(List<BodyItem> body, Span span) { Body = body; Span = span; }
    }
    public sealed class SetDirective : TransformDirective
    {
        public List<Attribute> Attrs { get; }
        public Span Span { get; }
        public SetDirective(List<Attribute> attrs, Span span) { Attrs = attrs; Span = span; }
    }
    public sealed class RemoveDirective : TransformDirective
    {
        public List<Ident> Names { get; }
        public Span Span { get; }
        public RemoveDirective(List<Ident> names, Span span) { Names = names; Span = span; }
    }
    public sealed class WhenDirective : TransformDirective
    {
        public Expr Condition { get; }
        public List<TransformDirective> Directives { get; }
        public Span Span { get; }
        public WhenDirective(Expr condition, List<TransformDirective> directives, Span span)
        { Condition = condition; Directives = directives; Span = span; }
    }

    // Macro calls
    public class MacroCall
    {
        public Ident Name { get; set; }
        public List<MacroCallArg> Args { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public MacroCall(Ident name, List<MacroCallArg> args, Trivia trivia, Span span)
        { Name = name; Args = args; Trivia = trivia; Span = span; }
    }

    public abstract class MacroCallArg { }
    public sealed class PositionalMacroArg : MacroCallArg
    {
        public Expr Value { get; }
        public PositionalMacroArg(Expr value) => Value = value;
    }
    public sealed class NamedMacroArg : MacroCallArg
    {
        public Ident Name { get; }
        public Expr Value { get; }
        public NamedMacroArg(Ident name, Expr value) { Name = name; Value = value; }
    }

    // Control flow
    public class ForLoop
    {
        public Ident Iterator { get; set; }
        public Ident? Index { get; set; }
        public Expr Iterable { get; set; }
        public List<BodyItem> Body { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public ForLoop(Ident iterator, Ident? index, Expr iterable,
                       List<BodyItem> body, Trivia trivia, Span span)
        { Iterator = iterator; Index = index; Iterable = iterable; Body = body; Trivia = trivia; Span = span; }
    }

    public class Conditional
    {
        public Expr Condition { get; set; }
        public List<BodyItem> ThenBody { get; set; }
        public ElseBranch? ElseBranch { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Conditional(Expr condition, List<BodyItem> thenBody, ElseBranch? elseBranch,
                           Trivia trivia, Span span)
        { Condition = condition; ThenBody = thenBody; ElseBranch = elseBranch; Trivia = trivia; Span = span; }
    }

    public abstract class ElseBranch { }
    public sealed class ElseIfBranch : ElseBranch
    {
        public Conditional Conditional { get; }
        public ElseIfBranch(Conditional conditional) => Conditional = conditional;
    }
    public sealed class ElseBlock : ElseBranch
    {
        public List<BodyItem> Body { get; }
        public Trivia Trivia { get; }
        public Span Span { get; }
        public ElseBlock(List<BodyItem> body, Trivia trivia, Span span)
        { Body = body; Trivia = trivia; Span = span; }
    }

    // Validation
    public class Validation
    {
        public List<Decorator> Decorators { get; set; }
        public StringLit Name { get; set; }
        public List<LetBinding> Lets { get; set; }
        public Expr Check { get; set; }
        public Expr Message { get; set; }
        public Trivia Trivia { get; set; }
        public Span Span { get; set; }
        public Validation(List<Decorator> decorators, StringLit name, List<LetBinding> lets,
                          Expr check, Expr message, Trivia trivia, Span span)
        {
            Decorators = decorators; Name = name; Lets = lets;
            Check = check; Message = message; Trivia = trivia; Span = span;
        }
    }
}
