use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::eval::value::Value;
use crate::transform::codec::custom::{self, CustomCodecRegistry};
use crate::transform::codec::native::OutputTarget;
use crate::transform::codec::CodecOptions;
use crate::transform::TransformError;
use crate::wdoc::model::{self, WdocDocument};

pub const HTML_CODEC: &str = "wdoc-html";

pub fn encode_html_document(
    doc: &WdocDocument,
    output: &Path,
    asset_dirs: &[PathBuf],
) -> Result<usize, String> {
    encode_document(doc, HTML_CODEC, output, asset_dirs)
}

pub fn encode_document(
    doc: &WdocDocument,
    codec_name: &str,
    output: &Path,
    asset_dirs: &[PathBuf],
) -> Result<usize, String> {
    let mut value = model::document_to_value(doc)?;
    add_wdoc_codec_support_values(&mut value)?;
    let registry = Arc::new(custom_registry()?);
    let codec = registry
        .get(codec_name)
        .ok_or_else(|| format!("unknown wdoc codec: {codec_name}"))?
        .clone();
    let options = asset_dir_options(asset_dirs);
    let written = custom::encode_custom_value_with_registry_and_builtins(
        &value,
        &codec,
        &options,
        OutputTarget::Directory(output),
        registry,
        crate::wdoc::source::wdoc_functions().functions,
    )
    .map_err(|e| e.to_string())?;
    let asset_refs: Vec<&Path> = asset_dirs.iter().map(|path| path.as_path()).collect();
    crate::wdoc::render::finalize_assets(output, &asset_refs)?;
    if codec_name == HTML_CODEC {
        Ok(doc.pages.len())
    } else {
        Ok(written)
    }
}

fn add_wdoc_codec_support_values(value: &mut Value) -> Result<(), String> {
    let Value::Map(map) = value else {
        return Ok(());
    };
    map.insert(
        "runtime".to_string(),
        Value::Map(indexmap::IndexMap::from([
            (
                "mathjax_config".to_string(),
                Value::String(crate::wdoc::assets::mathjax_config_js()?.to_string()),
            ),
            (
                "theme".to_string(),
                Value::String(crate::wdoc::assets::theme_runtime_js()?.to_string()),
            ),
            (
                "presentation".to_string(),
                Value::String(crate::wdoc::assets::presentation_runtime_js()?.to_string()),
            ),
            (
                "page_signal_template".to_string(),
                Value::String(crate::wdoc::assets::page_signal_runtime_template_js()?.to_string()),
            ),
        ])),
    );
    map.insert(
        "base_styles".to_string(),
        Value::String(crate::wdoc::assets::base_css()?),
    );
    Ok(())
}

pub(crate) fn custom_registry() -> Result<CustomCodecRegistry, String> {
    let mut registry = custom::standard_registry().map_err(|e| e.to_string())?;
    let doc = parse_wdoc_library_for_codecs()?;
    let wdoc_registry = custom::registry_from_document(&doc, true).map_err(|e| e.to_string())?;
    let codec = wdoc_registry
        .get(HTML_CODEC)
        .ok_or_else(|| format!("bundled WDoc codec '{HTML_CODEC}' was not found"))?
        .clone();
    registry.insert_standard(codec).map_err(|e| e.to_string())?;
    Ok(registry)
}

fn asset_dir_options(asset_dirs: &[PathBuf]) -> CodecOptions {
    let mut options = CodecOptions::new();
    options.insert(
        "asset_dirs".to_string(),
        Value::List(
            asset_dirs
                .iter()
                .map(|path| Value::String(path.display().to_string()))
                .collect(),
        ),
    );
    options
}

pub(crate) fn asset_dirs_from_options(
    options: &CodecOptions,
) -> Result<Vec<PathBuf>, TransformError> {
    let Some(value) = options.get("asset_dirs") else {
        return Ok(Vec::new());
    };
    let Value::List(items) = value else {
        return Err(TransformError::Codec(
            "wdoc-html option 'asset_dirs' must be a list of paths".to_string(),
        ));
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(path) | Value::Identifier(path) => Ok(PathBuf::from(path)),
            other => Err(TransformError::Codec(format!(
                "wdoc-html option 'asset_dirs' entries must be paths, got {}",
                other.type_name()
            ))),
        })
        .collect()
}

pub(crate) fn finalize_html_assets_from_options(
    output: &Path,
    options: &CodecOptions,
) -> Result<(), TransformError> {
    let asset_dirs = asset_dirs_from_options(options)?;
    let asset_refs: Vec<&Path> = asset_dirs.iter().map(|path| path.as_path()).collect();
    crate::wdoc::render::finalize_assets(output, &asset_refs).map_err(TransformError::Codec)
}

fn parse_wdoc_library_for_codecs() -> Result<crate::Document, String> {
    let doc = crate::parse(
        crate::standard_lib::WDOC_LIBRARY_WCL,
        crate::ParseOptions {
            root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
            functions: crate::wdoc::source::wdoc_functions(),
            ..Default::default()
        },
    );
    if doc.has_errors() {
        let errors = doc
            .errors()
            .into_iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("failed to parse bundled WDoc codecs: {errors}"));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wdoc::model::{
        ContentBlock, Layout, LayoutItem, Page, Section, SiteConfig, WdocTemplate,
    };

    fn minimal_doc() -> WdocDocument {
        WdocDocument {
            name: "docs".to_string(),
            title: "Docs".to_string(),
            template: WdocTemplate::Book,
            version: None,
            author: None,
            site: SiteConfig::default(),
            sections: vec![Section {
                id: "docs.home".to_string(),
                short_id: "home".to_string(),
                title: "Home".to_string(),
                children: vec![],
            }],
            pages: vec![Page {
                id: "home".to_string(),
                section_id: "docs.home".to_string(),
                title: "Home".to_string(),
                template: None,
                path: None,
                date: None,
                draft: false,
                weight: None,
                summary: None,
                tags: vec![],
                categories: vec![],
                params: Default::default(),
                layout: Layout {
                    children: vec![LayoutItem::Content(ContentBlock {
                        kind: "paragraph".to_string(),
                        id: Some("intro".to_string()),
                        rendered_html: "<p>Hello</p>".to_string(),
                        style: None,
                    })],
                },
                signals: vec![],
                bindings: vec![],
            }],
            styles: vec![],
            extra_css: String::new(),
        }
    }

    #[test]
    fn wdoc_html_codec_writes_site_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("out");
        let doc = minimal_doc();
        let output_for_thread = output.clone();
        let written = std::thread::Builder::new()
            .name("wdoc-html-codec-test".to_string())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || encode_html_document(&doc, &output_for_thread, &[]))
            .expect("spawn")
            .join()
            .expect("join")
            .expect("encode");

        assert_eq!(written, 1);
        let html = std::fs::read_to_string(output.join("home.html")).expect("home.html");
        assert!(html.contains("<p>Hello</p>"));
    }

    #[test]
    fn wdoc_document_value_round_trips() {
        let doc = minimal_doc();
        let value = model::document_to_value(&doc).expect("to value");
        let decoded = model::document_from_value(&value).expect("from value");

        assert_eq!(decoded.name, doc.name);
        assert_eq!(decoded.pages[0].id, "home");
        assert_eq!(decoded.pages[0].layout.children.len(), 1);
    }
}
