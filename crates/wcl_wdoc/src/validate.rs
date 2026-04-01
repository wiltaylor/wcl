use crate::model::*;

/// Severity of a validation diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WdocSeverity {
    Error,
    Warning,
}

/// A validation diagnostic.
#[derive(Debug, Clone)]
pub struct WdocDiagnostic {
    pub severity: WdocSeverity,
    pub message: String,
}

impl std::fmt::Display for WdocDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            WdocSeverity::Error => "error",
            WdocSeverity::Warning => "warning",
        };
        write!(f, "{prefix}: {}", self.message)
    }
}

/// Validate a `WdocDocument` for semantic correctness.
pub fn validate(doc: &WdocDocument) -> Vec<WdocDiagnostic> {
    let mut diags = Vec::new();

    // Collect all valid section paths
    let mut section_paths = Vec::new();
    collect_section_paths(&doc.sections, &mut section_paths);

    // Validate pages reference valid sections
    for page in &doc.pages {
        if !section_paths.contains(&page.section_id) {
            diags.push(WdocDiagnostic {
                severity: WdocSeverity::Error,
                message: format!(
                    "page '{}' references unknown section '{}' (available: {})",
                    page.id,
                    page.section_id,
                    section_paths.join(", ")
                ),
            });
        }
    }

    // Validate layouts
    for page in &doc.pages {
        validate_layout_items(&page.layout.children, &page.id, &mut diags);
    }

    // Validate style references
    let style_names: Vec<&str> = doc.styles.iter().map(|s| s.name.as_str()).collect();
    for page in &doc.pages {
        validate_style_refs(&page.layout.children, &style_names, &page.id, &mut diags);
    }

    diags
}

fn collect_section_paths(sections: &[Section], paths: &mut Vec<String>) {
    for section in sections {
        paths.push(section.id.clone());
        collect_section_paths(&section.children, paths);
    }
}

fn validate_layout_items(items: &[LayoutItem], page_id: &str, diags: &mut Vec<WdocDiagnostic>) {
    for item in items {
        match item {
            LayoutItem::SplitGroup(group) => {
                let total: f64 = group.splits.iter().map(|s| s.size_percent).sum();
                if (total - 100.0).abs() > 0.01 && !group.splits.is_empty() {
                    diags.push(WdocDiagnostic {
                        severity: WdocSeverity::Warning,
                        message: format!(
                            "page '{page_id}': split sizes sum to {total:.0}%, expected 100%"
                        ),
                    });
                }
                for split in &group.splits {
                    validate_layout_items(&split.children, page_id, diags);
                }
            }
            LayoutItem::Content(_) => {
                // Content blocks are validated by their schema + template function.
                // No hardcoded content-type-specific checks here.
            }
        }
    }
}

fn validate_style_refs(
    items: &[LayoutItem],
    style_names: &[&str],
    page_id: &str,
    diags: &mut Vec<WdocDiagnostic>,
) {
    for item in items {
        match item {
            LayoutItem::SplitGroup(group) => {
                for split in &group.splits {
                    validate_style_refs(&split.children, style_names, page_id, diags);
                }
            }
            LayoutItem::Content(content) => {
                if let Some(name) = &content.style {
                    if !style_names.contains(&name.as_str()) {
                        diags.push(WdocDiagnostic {
                            severity: WdocSeverity::Error,
                            message: format!(
                                "page '{page_id}': @style(\"{name}\") references undefined style"
                            ),
                        });
                    }
                }
            }
        }
    }
}
