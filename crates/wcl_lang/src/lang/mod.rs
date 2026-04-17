//! WCL Core — AST, lexer, parser, spans, trivia, diagnostics

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod trivia;

pub use diagnostic::{Diagnostic, DiagnosticBag, Label, Severity};
pub use span::{FileId, SourceFile, SourceMap, Span};
pub use trivia::{Comment, CommentPlacement, CommentStyle, Trivia};

// Re-export the most commonly used AST types.
pub use ast::{
    Attribute, BinOp, Block, BodyItem, CallArg, ColumnDecl, Conditional, Decorator, DecoratorArg,
    DecoratorSchema, DecoratorTarget, DocItem, Document, ElseBranch, ExportLet, Expr, ForLoop,
    Ident, IdentifierLit, Import, InjectBlock, InlineId, LetBinding, MacroBody, MacroCall,
    MacroCallArg, MacroDef, MacroKind, MacroParam, MapKey, PathSegment, QueryFilter, QueryPipeline,
    QuerySelector, ReExport, RemoveBlock, Schema, SchemaField, SetBlock, StringLit, StringPart,
    Table, TableRow, TransformDirective, TypeExpr, UnaryOp, Validation, WhenBlock,
};

pub use parser::Parser;

/// Parse a query pipeline from a standalone string, e.g. `"service | .port > 80"`.
///
/// Returns the parsed `QueryPipeline` or diagnostics on failure.
pub fn parse_query(source: &str, file_id: FileId) -> Result<ast::QueryPipeline, DiagnosticBag> {
    let tokens = match lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            let mut diags = DiagnosticBag::new();
            for d in lex_errors {
                diags.add(d);
            }
            return Err(diags);
        }
    };
    let parser = parser::Parser::new(tokens);
    let (pipeline_opt, diags) = parser.parse_query_standalone();
    if diags.has_errors() {
        return Err(diags);
    }
    match pipeline_opt {
        Some(pipeline) => Ok(pipeline),
        None => {
            let mut diags = diags;
            diags.error(
                "failed to parse query pipeline",
                Span::new(file_id, 0, source.len()),
            );
            Err(diags)
        }
    }
}

/// Parse a standalone WCL expression from a string.
///
/// Used by `wcl eval <file> <expression>` to parse the projection expression.
pub fn parse_expression(source: &str, file_id: FileId) -> Result<ast::Expr, DiagnosticBag> {
    let tokens = match lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            let mut diags = DiagnosticBag::new();
            for d in lex_errors {
                diags.add(d);
            }
            return Err(diags);
        }
    };
    let parser = parser::Parser::new(tokens);
    let (expr_opt, diags) = parser.parse_expr_standalone();
    if diags.has_errors() {
        return Err(diags);
    }
    match expr_opt {
        Some(expr) => Ok(expr),
        None => {
            let mut diags = diags;
            diags.error(
                "failed to parse expression",
                Span::new(file_id, 0, source.len()),
            );
            Err(diags)
        }
    }
}

/// Lex and parse WCL source text, returning the AST and accumulated diagnostics.
pub fn parse(source: &str, file_id: FileId) -> (ast::Document, DiagnosticBag) {
    let mut diags = DiagnosticBag::new();
    let tokens = match lexer::lex(source, file_id) {
        Ok(tokens) => tokens,
        Err(lex_errors) => {
            for d in lex_errors {
                diags.add(d);
            }
            // Return an empty document on lex failure
            let doc = ast::Document {
                items: Vec::new(),
                trivia: Trivia::empty(),
                span: Span::new(file_id, 0, source.len()),
            };
            return (doc, diags);
        }
    };
    let parser = parser::Parser::new(tokens);
    let (doc, parser_diags, _tokens) = parser.parse_document();
    diags.merge(parser_diags);
    (doc, diags)
}
