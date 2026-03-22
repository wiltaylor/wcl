use crate::eval::{FunctionSignature, MacroRegistry, ScopeArena};
use crate::lang::ast;
use crate::lang::diagnostic::Diagnostic;
use crate::lang::lexer::Token;
use crate::lang::span::{FileId, SourceMap};
use crate::schema::SchemaRegistry;
use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

pub struct DocumentState {
    pub uri: Url,
    pub version: i32,
    pub source: String,
    pub rope: Rope,
    pub analysis: Option<AnalysisResult>,
}

pub struct AnalysisResult {
    pub ast: ast::Document,
    pub tokens: Vec<Token>,
    pub source_map: SourceMap,
    pub file_id: FileId,
    pub diagnostics: Vec<Diagnostic>,
    pub values: indexmap::IndexMap<String, crate::eval::Value>,
    pub scopes: ScopeArena,
    pub schemas: SchemaRegistry,
    pub macro_registry: MacroRegistry,
    pub function_signatures: Vec<FunctionSignature>,
}

pub struct WorldState {
    pub documents: DashMap<Url, DocumentState>,
    pub default_options: crate::ParseOptions,
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldState {
    pub fn new() -> Self {
        WorldState {
            documents: DashMap::new(),
            default_options: crate::ParseOptions::default(),
        }
    }
}
