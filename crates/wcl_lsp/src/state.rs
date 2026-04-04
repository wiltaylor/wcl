use std::collections::HashMap;

use async_lsp::lsp_types::Url;
use ropey::Rope;
use wcl_lang::eval::{FunctionSignature, MacroRegistry, ScopeArena};
use wcl_lang::lang::ast;
use wcl_lang::lang::diagnostic::Diagnostic;
use wcl_lang::lang::lexer::Token;
use wcl_lang::lang::span::{FileId, SourceMap};
use wcl_lang::schema::SchemaRegistry;

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
    pub values: indexmap::IndexMap<String, wcl_lang::eval::Value>,
    pub scopes: ScopeArena,
    pub schemas: SchemaRegistry,
    pub macro_registry: MacroRegistry,
    pub function_signatures: Vec<FunctionSignature>,
}

pub struct WorldState {
    pub documents: HashMap<Url, DocumentState>,
    pub default_options: wcl_lang::ParseOptions,
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldState {
    pub fn new() -> Self {
        WorldState {
            documents: HashMap::new(),
            default_options: wcl_lang::ParseOptions::default(),
        }
    }
}
