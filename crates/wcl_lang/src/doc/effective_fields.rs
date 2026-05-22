//! Inheritance-aware field accessors.
//!
//! Two declarations participate in inheritance (`extends`): [`TypeDecl`] and
//! [`InterfaceDecl`]. Both expose `effective_fields()` / `effective_field()`
//! that walk the ancestor chain and de-duplicate by name. This module owns
//! the small recursive helpers behind those methods plus `is_descendant_of`.

use std::collections::HashSet;

use super::{Document, InterfaceDecl, TypeDecl, TypeField};

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
}

fn lookup_parent<'a>(doc: &'a Document, path: &[String]) -> Option<ParentDecl<'a>> {
    let key = path.join(".");
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
    own_fields: I,
) -> Vec<TypeField<'a>>
where
    I: IntoIterator<Item = TypeField<'a>>,
{
    let mut out: Vec<TypeField<'a>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    collect_effective_fields(doc, extends, &mut out, &mut seen);
    for f in own_fields {
        insert_or_override(&mut out, &mut seen, f);
    }
    out
}

/// Single-field lookup against the effective-field set. `own_lookup` checks
/// the declaration itself; if it misses, walk parents.
pub(super) fn lookup_effective_field<'a, F>(
    doc: &'a Document,
    extends: &[Vec<String>],
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
        if let Some(f) = effective_field_via(doc, parent_path, name) {
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
    out: &mut Vec<TypeField<'a>>,
    seen: &mut HashSet<String>,
) {
    for parent_path in extends_paths {
        if let Some(decl) = lookup_parent(doc, parent_path) {
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
    name: &str,
) -> Option<TypeField<'a>> {
    lookup_parent(doc, parent_path).and_then(|d| d.effective_field(name))
}

pub(super) fn is_descendant_of_walk(
    doc: &Document,
    extends_paths: &[Vec<String>],
    target_fqn: &str,
    seen: &mut HashSet<String>,
) -> bool {
    for parent_path in extends_paths {
        let key = parent_path.join(".");
        if !seen.insert(key.clone()) {
            continue;
        }
        if key == target_fqn {
            return true;
        }
        if let Some(decl) = lookup_parent(doc, parent_path)
            && is_descendant_of_walk(doc, decl.extends(), target_fqn, seen)
        {
            return true;
        }
    }
    false
}
