use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use wcl::{ResolvedField, ResolvedSchema, ResolvedVariant, ValidateConstraints};
use wcl_schema::type_name;

pub fn run(
    files: &[std::path::PathBuf],
    output: &Path,
    title: &str,
    lib_args: &crate::LibraryArgs,
) -> Result<(), String> {
    // Parse all files and collect schemas
    let mut all_schemas: HashMap<String, ResolvedSchema> = HashMap::new();

    for file in files {
        let source = fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {}", file.display(), e))?;
        let mut opts = wcl::ParseOptions {
            root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            ..Default::default()
        };
        lib_args.apply(&mut opts);
        let doc = wcl::parse(&source, opts);
        for (name, schema) in doc.schemas.schemas {
            all_schemas.insert(name, schema);
        }
    }

    if all_schemas.is_empty() {
        return Err("no schemas found in input files".to_string());
    }

    // Build hierarchy
    let tree = SchemaTree::build(&all_schemas);

    // Create output directories
    let src_dir = output.join("src");
    let schemas_dir = src_dir.join("schemas");
    fs::create_dir_all(&schemas_dir)
        .map_err(|e| format!("failed to create output directory: {}", e))?;

    // Write book.toml
    fs::write(
        output.join("book.toml"),
        format!(
            "[book]\ntitle = \"{}\"\nsrc = \"src\"\n\n[output.html]\ndefault-theme = \"light\"\n",
            title
        ),
    )
    .map_err(|e| format!("failed to write book.toml: {}", e))?;

    // Build ordered list for SUMMARY
    let ordered = tree.ordered_schemas();

    // Write SUMMARY.md
    let mut summary = String::from("# Summary\n\n");
    summary.push_str("[Overview](overview.md)\n\n");
    summary.push_str("# Schemas\n\n");
    for (name, depth) in &ordered {
        let indent = "  ".repeat(*depth);
        summary.push_str(&format!("{}- [{}](schemas/{}.md)\n", indent, name, name));
    }
    fs::write(src_dir.join("SUMMARY.md"), summary)
        .map_err(|e| format!("failed to write SUMMARY.md: {}", e))?;

    // Write overview.md
    let mut overview = format!("# {}\n\n", title);
    overview.push_str(&format!(
        "This reference documents **{}** schemas.\n\n",
        all_schemas.len()
    ));

    // Hierarchy diagram
    if !tree.roots.is_empty() {
        overview.push_str("## Hierarchy\n\n```\n");
        let mut diagram_visited = HashSet::new();
        for root in &tree.roots {
            write_tree_diagram(
                &mut overview,
                root,
                &tree.children_map,
                0,
                &mut diagram_visited,
            );
        }
        overview.push_str("```\n\n");
    }

    overview.push_str("## All Schemas\n\n");
    overview.push_str("| Schema | Description |\n|--------|-------------|\n");
    let mut sorted_names: Vec<_> = all_schemas.keys().collect();
    sorted_names.sort();
    for name in &sorted_names {
        let schema = &all_schemas[*name];
        let desc = schema.doc.as_deref().unwrap_or("");
        overview.push_str(&format!("| [{}](schemas/{}.md) | {} |\n", name, name, desc));
    }
    fs::write(src_dir.join("overview.md"), overview)
        .map_err(|e| format!("failed to write overview.md: {}", e))?;

    // Write per-schema pages
    for (name, schema) in &all_schemas {
        let page = render_schema_page(name, schema, &all_schemas);
        fs::write(schemas_dir.join(format!("{}.md", name)), page)
            .map_err(|e| format!("failed to write {}.md: {}", name, e))?;
    }

    eprintln!(
        "Generated mdBook with {} schemas in {}",
        all_schemas.len(),
        output.display()
    );
    Ok(())
}

// ── Schema tree for hierarchy ───────────────────────────────────────────────

struct SchemaTree {
    roots: Vec<String>,
    children_map: HashMap<String, Vec<String>>,
}

impl SchemaTree {
    fn build(schemas: &HashMap<String, ResolvedSchema>) -> Self {
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut has_parent: HashSet<String> = HashSet::new();

        for (name, schema) in schemas {
            if name == "_root" {
                continue;
            }
            if let Some(ref parents) = schema.allowed_parents {
                for parent in parents {
                    children_map
                        .entry(parent.clone())
                        .or_default()
                        .push(name.clone());
                    has_parent.insert(name.clone());
                }
            }
        }

        // Sort children for deterministic output
        for children in children_map.values_mut() {
            children.sort();
        }

        // Roots: schemas listed as children of _root, or schemas without @parent
        let mut roots: Vec<String> = Vec::new();
        if let Some(root_children) = children_map.get("_root") {
            roots.extend(root_children.iter().cloned());
        }
        // Add schemas with no @parent that aren't already roots
        let mut parentless: Vec<String> = schemas
            .keys()
            .filter(|n| *n != "_root" && !has_parent.contains(*n))
            .cloned()
            .collect();
        parentless.sort();
        for name in parentless {
            if !roots.contains(&name) {
                roots.push(name);
            }
        }
        roots.sort();

        SchemaTree {
            roots,
            children_map,
        }
    }

    fn ordered_schemas(&self) -> Vec<(String, usize)> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        for root in &self.roots {
            self.dfs(root, 0, &mut visited, &mut result);
        }
        result
    }

    fn dfs(
        &self,
        name: &str,
        depth: usize,
        visited: &mut HashSet<String>,
        result: &mut Vec<(String, usize)>,
    ) {
        if !visited.insert(name.to_string()) {
            return;
        }
        result.push((name.to_string(), depth));
        if let Some(children) = self.children_map.get(name) {
            for child in children {
                self.dfs(child, depth + 1, visited, result);
            }
        }
    }
}

fn write_tree_diagram(
    out: &mut String,
    name: &str,
    children_map: &HashMap<String, Vec<String>>,
    depth: usize,
    visited: &mut HashSet<String>,
) {
    let indent = "  ".repeat(depth);
    if !visited.insert(name.to_string()) {
        out.push_str(&format!("{}{} (...)\n", indent, name));
        return;
    }
    out.push_str(&format!("{}{}\n", indent, name));
    if let Some(children) = children_map.get(name) {
        for child in children {
            write_tree_diagram(out, child, children_map, depth + 1, visited);
        }
    }
}

// ── Page rendering ──────────────────────────────────────────────────────────

fn render_schema_page(
    name: &str,
    schema: &ResolvedSchema,
    all_schemas: &HashMap<String, ResolvedSchema>,
) -> String {
    let mut page = format!("# {}\n\n", name);

    // Doc text
    if let Some(ref doc) = schema.doc {
        page.push_str(doc);
        page.push_str("\n\n");
    }

    // Badges
    let mut badges = Vec::new();
    if schema.open {
        badges.push("`open`".to_string());
    }
    if let Some(ref tag) = schema.tag_field {
        badges.push(format!("`tagged({})`", tag));
    }
    if schema
        .allowed_children
        .as_ref()
        .is_some_and(|c| c.is_empty())
    {
        badges.push("`leaf`".to_string());
    }
    if !badges.is_empty() {
        page.push_str(&badges.join(" "));
        page.push_str("\n\n");
    }

    // Fields table
    if !schema.fields.is_empty() {
        page.push_str("## Fields\n\n");
        page.push_str(
            "| Field | Type | Required | Default | Constraints | Description |\n\
             |-------|------|----------|---------|-------------|-------------|\n",
        );
        for field in &schema.fields {
            page.push_str(&render_field_row(field));
        }
        page.push('\n');
    }

    // Variants
    if !schema.variants.is_empty() {
        page.push_str("## Variants\n\n");
        for variant in &schema.variants {
            render_variant(&mut page, variant);
        }
    }

    // Relationships
    page.push_str("## Relationships\n\n");
    render_relationships(&mut page, name, schema, all_schemas);

    page
}

fn render_field_row(field: &ResolvedField) -> String {
    let type_str = type_name(&field.type_expr);
    let required = if field.required { "yes" } else { "no" };
    let default = field
        .default
        .as_ref()
        .map(|v| format!("`{}`", v))
        .unwrap_or_default();
    let constraints = render_constraints(&field.validate, &field.ref_target, &field.id_pattern);
    let desc = field.doc.as_deref().unwrap_or("");

    format!(
        "| {} | `{}` | {} | {} | {} | {} |\n",
        field.name, type_str, required, default, constraints, desc
    )
}

fn render_constraints(
    validate: &Option<ValidateConstraints>,
    ref_target: &Option<String>,
    id_pattern: &Option<String>,
) -> String {
    let mut parts = Vec::new();

    if let Some(ref v) = validate {
        if let Some(min) = v.min {
            parts.push(format!("min={}", min));
        }
        if let Some(max) = v.max {
            parts.push(format!("max={}", max));
        }
        if let Some(ref pat) = v.pattern {
            parts.push(format!("pattern=`{}`", pat));
        }
        if let Some(ref vals) = v.one_of {
            let items: Vec<String> = vals.iter().map(|v| format!("{}", v)).collect();
            parts.push(format!("one\\_of=[{}]", items.join(", ")));
        }
    }

    if let Some(ref target) = ref_target {
        parts.push(format!("@ref({})", target));
    }
    if let Some(ref pat) = id_pattern {
        parts.push(format!("@id\\_pattern(`{}`)", pat));
    }

    parts.join(", ")
}

fn render_variant(page: &mut String, variant: &ResolvedVariant) {
    page.push_str(&format!("### Variant: `{}`\n\n", variant.tag_value));
    if let Some(ref doc) = variant.doc {
        page.push_str(doc);
        page.push_str("\n\n");
    }
    if !variant.fields.is_empty() {
        page.push_str(
            "| Field | Type | Required | Default | Constraints | Description |\n\
             |-------|------|----------|---------|-------------|-------------|\n",
        );
        for field in &variant.fields {
            page.push_str(&render_field_row(field));
        }
        page.push('\n');
    }
}

fn render_relationships(
    page: &mut String,
    name: &str,
    schema: &ResolvedSchema,
    all_schemas: &HashMap<String, ResolvedSchema>,
) {
    // Parents
    match &schema.allowed_parents {
        Some(parents) => {
            let links: Vec<String> = parents
                .iter()
                .map(|p| {
                    if p == "_root" {
                        "*(root)*".to_string()
                    } else {
                        format!("[{}](schemas/{}.md)", p, p)
                    }
                })
                .collect();
            page.push_str(&format!("- **Parent**: {}\n", links.join(", ")));
        }
        None => page.push_str("- **Parent**: any\n"),
    }

    // Children
    match &schema.allowed_children {
        Some(children) if children.is_empty() => {
            page.push_str("- **Children**: none (leaf)\n");
        }
        Some(children) => {
            let links: Vec<String> = children
                .iter()
                .map(|c| format!("[{}](schemas/{}.md)", c, c))
                .collect();
            page.push_str(&format!("- **Children**: {}\n", links.join(", ")));
        }
        None => page.push_str("- **Children**: any\n"),
    }

    // Child constraints
    if !schema.child_constraints.is_empty() {
        page.push_str("- **Child constraints**:\n");
        for cc in &schema.child_constraints {
            let mut parts = vec![format!("`{}`", cc.kind)];
            if let Some(min) = cc.min {
                parts.push(format!("min={}", min));
            }
            if let Some(max) = cc.max {
                parts.push(format!("max={}", max));
            }
            if let Some(md) = cc.max_depth {
                parts.push(format!("max\\_depth={}", md));
            }
            page.push_str(&format!("  - {}\n", parts.join(", ")));
        }
    }

    // Reverse: who lists this schema as a child?
    let mut referenced_by: Vec<&str> = all_schemas
        .iter()
        .filter(|(_, s)| {
            s.allowed_children
                .as_ref()
                .is_some_and(|c| c.iter().any(|ch| ch == name))
        })
        .map(|(n, _)| n.as_str())
        .collect();
    referenced_by.sort();
    if !referenced_by.is_empty() {
        let links: Vec<String> = referenced_by
            .iter()
            .map(|r| {
                if *r == "_root" {
                    "*(root)*".to_string()
                } else {
                    format!("[{}](schemas/{}.md)", r, r)
                }
            })
            .collect();
        page.push_str(&format!("- **Referenced by**: {}\n", links.join(", ")));
    }
}
