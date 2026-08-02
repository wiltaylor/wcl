//! Inheritance-aware field accessors.
//!
//! Two declarations participate in inheritance (`extends`): [`TypeDecl`] and
//! [`InterfaceDecl`]. Both expose `effective_fields()` / `effective_field()`
//! that walk the ancestor chain and de-duplicate by name. This module owns
//! the small recursive helpers behind those methods plus `is_descendant_of`.

use std::collections::{HashMap, HashSet};

use super::{DeclName, Decorator, Document, InterfaceDecl, TypeDecl, TypeField};

/// A resolved `extends` target. `type` and `interface` declarations
/// share the same inheritance entry-points (`effective_fields`,
/// `effective_field`, `extends`); this wrapper keeps the rest of this
/// module out of the type-vs-interface match.
enum ParentDecl<'a> {
    Type(TypeDecl<'a>),
    Interface(InterfaceDecl<'a>),
}

impl<'a> ParentDecl<'a> {
    fn effective_fields(&self) -> Vec<TypeField<'a>> {
        match self {
            ParentDecl::Type(d) => d.effective_fields(),
            ParentDecl::Interface(d) => d.effective_fields(),
        }
    }

    fn effective_field(&self, name: &str) -> Option<TypeField<'a>> {
        match self {
            ParentDecl::Type(d) => d.effective_field(name),
            ParentDecl::Interface(d) => d.effective_field(name),
        }
    }

    fn extends(&self) -> &'a [Vec<String>] {
        match self {
            ParentDecl::Type(d) => &d.ast.extends,
            ParentDecl::Interface(d) => &d.ast.extends,
        }
    }

    /// The namespace this parent declaration lives in — used to resolve
    /// *its own* `extends` references when the walk recurses.
    fn file_ns(&self) -> &'a [String] {
        match self {
            ParentDecl::Type(d) => d.file_ns(),
            ParentDecl::Interface(d) => d.file_ns(),
        }
    }

    fn full_name(&self) -> String {
        match self {
            ParentDecl::Type(d) => d.full_name(),
            ParentDecl::Interface(d) => d.full_name(),
        }
    }
}

/// Resolve an `extends` path written in a source whose namespace is
/// `file_ns` to its declaration. The path resolves **within `file_ns`
/// first** (then the document's aliases/wildcards, then absolute), so a
/// stdlib type's bare `extends ContentBlock` under `namespace wdoc` finds
/// `wdoc.ContentBlock`.
fn lookup_parent<'a>(
    doc: &'a Document,
    path: &[String],
    file_ns: &[String],
) -> Option<ParentDecl<'a>> {
    let fqn = doc
        .resolve_path_in(path, file_ns)
        .unwrap_or_else(|| path.to_vec());
    let key = fqn.join(".");
    if let Some(d) = doc.type_decl(&key) {
        return Some(ParentDecl::Type(d));
    }
    doc.interface(&key).map(ParentDecl::Interface)
}

/// Build the effective-field list for a declaration: ancestors (transitively,
/// in extends-list order, with later overriding earlier on name) followed by
/// the declaration's own fields. Shared by `TypeDecl` and `InterfaceDecl`.
pub(super) fn build_effective_fields<'a, I>(
    doc: &'a Document,
    extends: &[Vec<String>],
    file_ns: &[String],
    own_fields: I,
) -> Vec<TypeField<'a>>
where
    I: IntoIterator<Item = TypeField<'a>>,
{
    let mut out: Vec<TypeField<'a>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    collect_effective_fields(doc, extends, file_ns, &mut out, &mut seen);
    for f in own_fields {
        insert_or_override(&mut out, &mut seen, f);
    }
    out
}

/// Build a `field name → merged decorators` map across the inheritance
/// chain. A field's decorators are its own first, then any from a same-named
/// ancestor field whose decorator name isn't already present (own wins
/// per-decorator). This lets a concrete type redeclare an interface field
/// (required for instance validation) while still inheriting the
/// interface's `@doc` / `@hidden` — so shared field documentation lives in
/// one place. Field names contributed only by ancestors are included too.
pub(super) fn build_merged_decorators<'a, I>(
    doc: &'a Document,
    extends: &[Vec<String>],
    file_ns: &[String],
    own_fields: I,
) -> HashMap<String, Vec<Decorator<'a>>>
where
    I: IntoIterator<Item = TypeField<'a>>,
{
    let mut map: HashMap<String, Vec<Decorator<'a>>> = HashMap::new();
    for f in own_fields {
        map.insert(f.name().to_string(), f.decorators().collect());
    }
    for parent_path in extends {
        if let Some(decl) = lookup_parent(doc, parent_path, file_ns) {
            for f in decl.effective_fields() {
                let entry = map.entry(f.name().to_string()).or_default();
                for d in f.decorators() {
                    if !entry.iter().any(|e| e.full_name() == d.full_name()) {
                        entry.push(d);
                    }
                }
            }
        }
    }
    map
}

/// Single-field lookup against the effective-field set. `own_lookup` checks
/// the declaration itself; if it misses, walk parents.
pub(super) fn lookup_effective_field<'a, F>(
    doc: &'a Document,
    extends: &[Vec<String>],
    file_ns: &[String],
    own_lookup: F,
    name: &str,
) -> Option<TypeField<'a>>
where
    F: FnOnce(&str) -> Option<TypeField<'a>>,
{
    if let Some(f) = own_lookup(name) {
        return Some(f);
    }
    for parent_path in extends {
        if let Some(f) = effective_field_via(doc, parent_path, file_ns, name) {
            return Some(f);
        }
    }
    None
}

/// Walk an extends path list and append each parent's effective
/// fields into `out`, de-duplicated by name. Used by both
/// `TypeDecl::effective_fields` and `InterfaceDecl::effective_fields`.
fn collect_effective_fields<'a>(
    doc: &'a Document,
    extends_paths: &[Vec<String>],
    file_ns: &[String],
    out: &mut Vec<TypeField<'a>>,
    seen: &mut HashSet<String>,
) {
    for parent_path in extends_paths {
        if let Some(decl) = lookup_parent(doc, parent_path, file_ns) {
            for f in decl.effective_fields() {
                insert_or_override(out, seen, f);
            }
        }
    }
}

fn insert_or_override<'a>(
    out: &mut Vec<TypeField<'a>>,
    seen: &mut HashSet<String>,
    f: TypeField<'a>,
) {
    if seen.contains(f.name()) {
        if let Some(slot) = out.iter_mut().find(|x| x.name() == f.name()) {
            *slot = f;
        }
    } else {
        seen.insert(f.name().to_string());
        out.push(f);
    }
}

fn effective_field_via<'a>(
    doc: &'a Document,
    parent_path: &[String],
    file_ns: &[String],
    name: &str,
) -> Option<TypeField<'a>> {
    lookup_parent(doc, parent_path, file_ns).and_then(|d| d.effective_field(name))
}

pub(super) fn is_descendant_of_walk(
    doc: &Document,
    extends_paths: &[Vec<String>],
    file_ns: &[String],
    target_fqn: &str,
    seen: &mut HashSet<String>,
) -> bool {
    for parent_path in extends_paths {
        let Some(decl) = lookup_parent(doc, parent_path, file_ns) else {
            // Unresolvable parent — fall back to a literal path compare so
            // a dangling `extends Foo` still matches `target_fqn == "Foo"`.
            if parent_path.join(".") == target_fqn {
                return true;
            }
            continue;
        };
        // Compare the *resolved* fully-qualified name, so a bare
        // `extends ContentBlock` under `namespace wdoc` matches a
        // `target_fqn` of `wdoc.ContentBlock`.
        let key = decl.full_name();
        if !seen.insert(key.clone()) {
            continue;
        }
        if key == target_fqn {
            return true;
        }
        // Recurse using the parent's own namespace to resolve its
        // `extends` references.
        if is_descendant_of_walk(doc, decl.extends(), decl.file_ns(), target_fqn, seen) {
            return true;
        }
    }
    false
}
