use std::path::{Path, PathBuf};

use crate::eval::value::Value;
use crate::transform::codec::native::{self, OutputTarget};
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
    let value = model::document_to_value(doc)?;
    let registry = native::NativeCodecRegistry::standard();
    let codec = registry
        .get(codec_name)
        .ok_or_else(|| format!("unknown native wdoc codec: {codec_name}"))?;
    let options = asset_dir_options(asset_dirs);
    native::encode_native_value(&value, codec, &options, OutputTarget::Directory(output))
        .map_err(|e| e.to_string())
}

pub(crate) fn encode_html_value(
    value: &Value,
    options: &CodecOptions,
    target: OutputTarget<'_>,
) -> Result<usize, TransformError> {
    let OutputTarget::Directory(output) = target else {
        return Err(TransformError::Codec(
            "wdoc-html codec writes a multi-file site and requires a directory output target"
                .to_string(),
        ));
    };
    let doc = model::document_from_value(value).map_err(TransformError::Codec)?;
    let asset_dirs = asset_dirs_from_options(options)?;
    let asset_refs: Vec<&Path> = asset_dirs.iter().map(|path| path.as_path()).collect();
    crate::wdoc::render::render_document(&doc, output, &asset_refs)
        .map_err(TransformError::Codec)?;
    Ok(doc.pages.len())
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

fn asset_dirs_from_options(options: &CodecOptions) -> Result<Vec<PathBuf>, TransformError> {
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
        let written = encode_html_document(&minimal_doc(), &output, &[]).expect("encode");

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
