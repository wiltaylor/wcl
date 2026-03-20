use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;
use wcl_core::ast;
use wcl_core::diagnostic::Diagnostic;
use wcl_core::lexer::Token;
use wcl_core::span::{FileId, SourceMap};
use wcl_eval::{FunctionSignature, MacroRegistry, ScopeArena};
use wcl_schema::SchemaRegistry;

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
    pub values: indexmap::IndexMap<String, wcl_eval::Value>,
    pub scopes: ScopeArena,
    pub schemas: SchemaRegistry,
    pub macro_registry: MacroRegistry,
    pub function_signatures: Vec<FunctionSignature>,
}

pub struct WorldState {
    pub documents: DashMap<Url, DocumentState>,
    pub default_options: wcl::ParseOptions,
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
            default_options: wcl::ParseOptions::default(),
        }
    }
}
