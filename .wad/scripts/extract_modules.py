#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = ["tree-sitter==0.25.*", "tree-sitter-rust==0.24.*"]
# ///
"""Extract the workspace's public code surface into
data/generated/modules.wcl, parsed with tree-sitter-rust (not regex).

Two tiers of code_items per workspace crate:

  1. One :module_graph item per crate (container-attached): a node per
     top-level module with `use`-derived dependency edges, plus one
     extern-crate stub node per workspace crate it imports — cross-crate
     connectivity renders here (each code_item draws its own diagram, so
     edges can't leave an item).
  2. :module_api items — the public view of the code: one code_node per
     pub type (struct / enum / trait) plus a per-module "fns" node for free
     functions, type aliases, and consts; one code_member row per member
     (field / variant / method / fn) carrying its rendered signature.
     A member's `deps` name the code_nodes its signature references, so
     the renderer draws an edge from the exact row to the type it mentions.
     Items are grouped onto components via COMPONENT_PREFIXES (longest
     module-path prefix wins); unmatched modules fall back to one item per
     top-level module attached to the crate's container.

Member deps only ever target nodes of the same code_item; a reference that
can't be resolved unambiguously (same module -> same item -> the module's
`use` imports -> unique across the workspace) is dropped, never guessed.

The wdoc stdlib component instead lists each lib/*.wcl part's declared
block kinds — its surface IS its block vocabulary."""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

import tree_sitter_rust
from tree_sitter import Language, Parser

WAD_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = WAD_ROOT.parent
OUT = WAD_ROOT / "data" / "generated" / "modules.wcl"

RUST = Language(tree_sitter_rust.language())
PARSER = Parser(RUST)

# Crate name -> WAD container id (same seam as extract_cargo.py). A crate
# missing here is skipped — extractors never invent architecture ids.
CRATE_TO_CONTAINER = {
    "wcl": "wcl_cli",
    "wcl_lang": "wcl_lang",
    "wcl_wdoc": "wcl_wdoc",
    "wcl_lsp": "wcl_lsp",
}

# Module-path prefix -> component id: the thin attribution seam. Longest
# prefix wins (matched on `::` boundaries), so deeper entries carve
# sub-trees out of a broader catch-all below them. Modules matching no
# prefix fall back to one code_item per top-level module, attached to the
# crate's container — new code is never silently dropped.
COMPONENT_PREFIXES = {
    # wcl_lang
    "wcl_lang::lexer": "lexer",
    "wcl_lang::parser": "parser",
    "wcl_lang::doc::eval": "evaluator",
    "wcl_lang::doc::eval_ops": "evaluator",
    "wcl_lang::doc::scope": "evaluator",
    "wcl_lang::doc::lookup": "evaluator",
    "wcl_lang::doc::match_pat": "evaluator",
    "wcl_lang::doc::schema_check": "schema_validator",
    "wcl_lang::doc::validate": "schema_validator",
    "wcl_lang::doc::variant_dispatch": "schema_validator",
    "wcl_lang::doc::loader": "registry",
    "wcl_lang::doc": "doc_view",
    "wcl_lang::format": "formatter",
    "wcl_lang::edit": "formatter",
    "wcl_lang::environment": "host_bindings",
    "wcl_lang::value": "host_bindings",
    "wcl_lang::reflect": "host_bindings",
    # wcl (the CLI binary)
    "wcl::scaffold": "cli_scaffold",
    "wcl::serve": "cli_serve",
    "wcl::edit": "cli_serve",
    "wcl::preview": "cli_serve",
    "wcl::gitspec": "cli_gitspec",
    "wcl": "cli_commands",
    # wcl_wdoc
    "wcl_wdoc::render": "wdoc_html_renderer",
    "wcl_wdoc::inline": "wdoc_html_renderer",
    "wcl_wdoc::highlight": "wdoc_html_renderer",
    "wcl_wdoc::layered": "wdoc_layout",
    "wcl_wdoc::force": "wdoc_layout",
    "wcl_wdoc::radial": "wdoc_layout",
    "wcl_wdoc::routing": "wdoc_layout",
    "wcl_wdoc::markdown": "wdoc_backends",
    "wcl_wdoc::pdf": "wdoc_backends",
}

# Diagram budgets: overflow becomes an explicit "… +N more" row/node, so a
# capped diagram SAYS it is capped (the code stays the source of truth).
MAX_NODES_PER_ITEM = 30
MAX_ROWS_PER_NODE = 14
MAX_SIG_CHARS = 64

SKIP_MODULES = {"tests"}

# The stdlib component: one node per lib part, members = its block kinds.
STDLIB_COMPONENT = "wdoc_stdlib"
STDLIB_GLOB = "crates/wcl_wdoc/lib/*.wcl"
WCL_BLOCK = re.compile(r'@block\("([a-z0-9_]+)"')

PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]')


# --------------------------------------------------------------------------
# Model
# --------------------------------------------------------------------------


@dataclass
class Member:
    id: str
    signature: str
    kind: str
    visibility: str | None  # None = pub, "crate" = pub(crate)
    ref_names: list[str] = field(default_factory=list)
    deps: list[str] = field(default_factory=list)


@dataclass
class TypeNode:
    id: str
    name: str
    kind: str  # struct / enum / trait / fns / module / extern_crate / overflow
    crate: str
    mod_path: tuple[str, ...]
    summary: str | None = None
    members: list[Member] = field(default_factory=list)
    deps: list[str] = field(default_factory=list)
    group: tuple | None = None


@dataclass
class Mod:
    crate: str
    path: tuple[str, ...]  # () = crate root
    file: Path
    body: "object"  # syntax node whose children are the module's items
    uses: list[tuple[str, ...]] = field(default_factory=list)  # absolute paths
    imports: dict[str, tuple[str, ...]] = field(default_factory=dict)
    pub_items: int = 0

    @property
    def path_str(self) -> str:
        return "::".join((self.crate,) + self.path)

    @property
    def display(self) -> str:
        return self.path[-1] if self.path else "crate root"

    @property
    def rel_file(self) -> str:
        return str(self.file.relative_to(REPO_ROOT / "crates" / self.crate))


# --------------------------------------------------------------------------
# Small helpers
# --------------------------------------------------------------------------


def wcl_str(s: str) -> str:
    out = s.replace("\\", "\\\\").replace('"', '\\"')
    out = out.replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")
    return f'"{out}"'


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9_]", "_", s)


def txt(node) -> str:
    return node.text.decode()


def collapse(s: str) -> str:
    return " ".join(s.split())


def clip(s: str) -> str:
    return s if len(s) <= MAX_SIG_CHARS else s[: MAX_SIG_CHARS - 1] + "…"


def visibility_of(node) -> str | None | bool:
    """None for pub, "crate" for pub(crate), False for anything narrower."""
    vis = next((c for c in node.children if c.type == "visibility_modifier"), None)
    if vis is None:
        return False
    t = collapse(txt(vis))
    if t == "pub":
        return None
    if t == "pub(crate)":
        return "crate"
    return False


def type_refs(nodes) -> list[str]:
    """All type_identifier names in the given subtrees (order kept, deduped)."""
    out: list[str] = []

    def go(n) -> None:
        if n.type == "type_identifier":
            name = txt(n)
            if name != "Self" and name not in out:
                out.append(name)
        for c in n.children:
            go(c)

    for n in nodes:
        if n is not None:
            go(n)
    return out


def preceding_attrs(node) -> list[str]:
    out = []
    sib = node.prev_named_sibling
    while sib is not None and sib.type in ("attribute_item", "line_comment", "block_comment"):
        if sib.type == "attribute_item":
            out.append(collapse(txt(sib)))
        sib = sib.prev_named_sibling
    return out


# --------------------------------------------------------------------------
# Module tree walking
# --------------------------------------------------------------------------


def child_file_dir(file: Path, path: tuple[str, ...]) -> Path:
    """Directory where `mod foo;` in `file` looks for foo.rs / foo/mod.rs."""
    if file.name in ("lib.rs", "main.rs", "mod.rs") or not path:
        return file.parent
    return file.parent / file.stem


def walk_crate(crate: str, root_file: Path) -> list[Mod]:
    mods: list[Mod] = []

    def walk(file: Path, path: tuple[str, ...], body) -> None:
        m = Mod(crate=crate, path=path, file=file, body=body)
        mods.append(m)
        for ch in body.children:
            if ch.type != "mod_item":
                continue
            name_node = ch.child_by_field_name("name")
            if name_node is None:
                continue
            name = txt(name_node)
            attrs = preceding_attrs(ch)
            if name in SKIP_MODULES or any("cfg(test" in a for a in attrs):
                continue
            inner = ch.child_by_field_name("body")
            if inner is not None:
                walk(file, path + (name,), inner)
                continue
            # `mod foo;` — resolve the file, honoring #[path = "…"].
            path_attr = next(
                (m2.group(1) for a in attrs if (m2 := PATH_ATTR.search(a))), None
            )
            base = child_file_dir(file, path)
            candidates = (
                [base / path_attr]
                if path_attr
                else [base / f"{name}.rs", base / name / "mod.rs"]
            )
            target = next((c for c in candidates if c.is_file()), None)
            if target is None:
                print(f"  warn: {file}: mod {name} not found", file=sys.stderr)
                continue
            tree = PARSER.parse(target.read_bytes())
            walk(target, path + (name,), tree.root_node)

    tree = PARSER.parse(root_file.read_bytes())
    walk(root_file, (), tree.root_node)
    return mods


def crate_roots() -> list[tuple[str, Path]]:
    out = []
    for crate_dir in sorted((REPO_ROOT / "crates").iterdir()):
        crate = crate_dir.name
        if crate not in CRATE_TO_CONTAINER:
            continue
        root = next(
            (p for p in (crate_dir / "src" / "lib.rs", crate_dir / "src" / "main.rs") if p.is_file()),
            None,
        )
        if root is not None:
            out.append((crate, root))
    return out


# --------------------------------------------------------------------------
# Use-declaration graph
# --------------------------------------------------------------------------


def flatten_use(node, prefix: tuple[str, ...]) -> list[tuple[str, ...]]:
    """One path tuple per leaf of a use tree (aliases keep the real path)."""
    t = node.type
    if t in ("identifier", "type_identifier", "crate", "super", "self", "metavariable"):
        return [prefix + (txt(node),)]
    if t == "scoped_identifier" or t == "scoped_use_list":
        path_node = node.child_by_field_name("path")
        base = flatten_use(path_node, prefix)[0] if path_node else prefix
        tail = node.child_by_field_name("name") or node.child_by_field_name("list")
        return flatten_use(tail, base) if tail else [base]
    if t == "use_list":
        out: list[tuple[str, ...]] = []
        for c in node.named_children:
            out.extend(flatten_use(c, prefix))
        return out
    if t == "use_as_clause":
        arg = node.child_by_field_name("path")
        return flatten_use(arg, prefix) if arg else []
    if t == "use_wildcard":
        inner = node.named_children[0] if node.named_children else None
        return [flatten_use(inner, prefix)[0] + ("*",)] if inner else []
    return []


def absolutize(path: tuple[str, ...], m: Mod) -> tuple[str, ...] | None:
    """Resolve crate/self/super heads to a crate-name-rooted absolute path."""
    if not path:
        return None
    head, rest = path[0], list(path[1:])
    if head == "crate":
        return (m.crate, *rest)
    if head == "self":
        return (m.crate, *m.path, *rest)
    if head == "super":
        up = list(m.path[:-1]) if m.path else []
        while rest and rest[0] == "super":
            rest.pop(0)
            up = up[:-1]
        return (m.crate, *up, *rest)
    return tuple(path)  # external or workspace crate name


def collect_uses(m: Mod) -> None:
    for ch in m.body.children:
        if ch.type != "use_declaration":
            continue
        arg = ch.child_by_field_name("argument")
        if arg is None:
            continue
        for p in flatten_use(arg, ()):
            ab = absolutize(p, m)
            if ab is None or ab[-1] == "*":
                continue
            m.uses.append(ab)
            m.imports.setdefault(ab[-1], ab)


# --------------------------------------------------------------------------
# Public-item extraction (tier 2)
# --------------------------------------------------------------------------


def fn_signature(node) -> tuple[str, list]:
    name = txt(node.child_by_field_name("name"))
    tp = node.child_by_field_name("type_parameters")
    params = node.child_by_field_name("parameters")
    ret = node.child_by_field_name("return_type")
    sig = f"fn {name}{collapse(txt(tp)) if tp else ''}{collapse(txt(params)) if params else '()'}"
    if ret is not None:
        sig += f" -> {collapse(txt(ret))}"
    return sig, [params, ret]


class Extractor:
    def __init__(self) -> None:
        self.groups: dict[tuple, list[TypeNode]] = {}
        self.group_order: list[tuple] = []
        # type name -> [(crate, mod_path, group_key, node_id)]
        self.index: dict[str, list[tuple[str, tuple[str, ...], tuple, str]]] = {}
        self.node_by_id: dict[str, TypeNode] = {}
        self.used_ids: set[str] = set()
        self.fns_nodes: dict[tuple, TypeNode] = {}  # (group, crate, mod_path)
        self.prefixes = sorted(COMPONENT_PREFIXES, key=len, reverse=True)

    def group_of(self, m: Mod) -> tuple:
        ps = m.path_str
        for pre in self.prefixes:
            if ps == pre or ps.startswith(pre + "::"):
                return ("comp", COMPONENT_PREFIXES[pre])
        return ("cont", m.crate, m.path[0] if m.path else "root")

    def fresh_id(self, base: str) -> str:
        cand, i = base, 2
        while cand in self.used_ids:
            cand = f"{base}_{i}"
            i += 1
        self.used_ids.add(cand)
        return cand

    def group_slug(self, key: tuple) -> str:
        return key[1] if key[0] == "comp" else f"{key[1]}_{key[2]}"

    def add_node(self, key: tuple, node: TypeNode, indexed: bool = True) -> TypeNode:
        if key not in self.groups:
            self.groups[key] = []
            self.group_order.append(key)
        node.group = key
        self.groups[key].append(node)
        self.node_by_id[node.id] = node
        if indexed:
            self.index.setdefault(node.name, []).append(
                (node.crate, node.mod_path, key, node.id)
            )
        return node

    def type_node(self, m: Mod, name: str, kind: str) -> TypeNode:
        key = self.group_of(m)
        nid = self.fresh_id(f"n_api_{self.group_slug(key)}_{slug(name)}")
        return self.add_node(
            key,
            TypeNode(id=nid, name=name, kind=kind, crate=m.crate, mod_path=m.path,
                     summary=m.rel_file),
        )

    def fns_node(self, m: Mod) -> TypeNode:
        key = self.group_of(m)
        fkey = (key, m.crate, m.path)
        if fkey not in self.fns_nodes:
            nid = self.fresh_id(f"n_api_{self.group_slug(key)}_{slug(m.display)}_fns")
            self.fns_nodes[fkey] = self.add_node(
                key,
                TypeNode(id=nid, name=m.display, kind="fns", crate=m.crate,
                         mod_path=m.path, summary=m.rel_file),
                indexed=False,
            )
        return self.fns_nodes[fkey]

    def member(self, node: TypeNode, name: str, signature: str, kind: str,
               visibility: str | None, refs: list[str]) -> None:
        mid = self.fresh_id(f"m_{node.id}_{slug(name)}")
        node.members.append(
            Member(id=mid, signature=clip(signature), kind=kind,
                   visibility=visibility, ref_names=refs)
        )

    # -- per-module extraction ------------------------------------------------

    def extract_module(self, m: Mod) -> None:
        for ch in m.body.children:
            t = ch.type
            if t == "struct_item":
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                node = self.type_node(m, txt(ch.child_by_field_name("name")), "struct")
                body = ch.child_by_field_name("body")
                if body is not None and body.type == "field_declaration_list":
                    for f in body.named_children:
                        if f.type != "field_declaration":
                            continue
                        fvis = visibility_of(f)
                        if fvis is False:
                            continue
                        fname = txt(f.child_by_field_name("name"))
                        ftype = f.child_by_field_name("type")
                        self.member(node, fname, f"{fname}: {collapse(txt(ftype))}",
                                    "field", fvis, type_refs([ftype]))
                elif body is not None and body.type == "ordered_field_declaration_list":
                    types = [c.child_by_field_name("type") for c in body.named_children
                             if c.type == "ordered_field_declaration"]
                    pub_types = [c.child_by_field_name("type") for c in body.named_children
                                 if c.type == "ordered_field_declaration"
                                 and visibility_of(c) is not False]
                    if pub_types:
                        sig = "(" + ", ".join(collapse(txt(ty)) for ty in types if ty) + ")"
                        self.member(node, "fields", sig, "field", None,
                                    type_refs(pub_types))
            elif t == "enum_item":
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                node = self.type_node(m, txt(ch.child_by_field_name("name")), "enum")
                body = ch.child_by_field_name("body")
                for v in (body.named_children if body else []):
                    if v.type != "enum_variant":
                        continue
                    vname = txt(v.child_by_field_name("name"))
                    vbody = v.child_by_field_name("body")
                    sig = vname + (collapse(txt(vbody)) if vbody is not None else "")
                    self.member(node, vname, sig, "variant", None,
                                type_refs([vbody]))
            elif t == "trait_item":
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                node = self.type_node(m, txt(ch.child_by_field_name("name")), "trait")
                body = ch.child_by_field_name("body")
                for it in (body.named_children if body else []):
                    if it.type in ("function_item", "function_signature_item"):
                        sig, refs = fn_signature(it)
                        self.member(node, txt(it.child_by_field_name("name")), sig,
                                    "method", None, type_refs(refs))
                    elif it.type == "associated_type":
                        aname = txt(it.child_by_field_name("name"))
                        self.member(node, aname, f"type {aname}", "assoc_type", None, [])
            elif t == "function_item":
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                sig, refs = fn_signature(ch)
                self.member(self.fns_node(m), txt(ch.child_by_field_name("name")),
                            sig, "fn", vis, type_refs(refs))
            elif t == "type_item":
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                tname = txt(ch.child_by_field_name("name"))
                rhs = ch.child_by_field_name("type")
                self.member(self.fns_node(m), tname,
                            f"type {tname} = {collapse(txt(rhs))}", "type", vis,
                            type_refs([rhs]))
                # An alias is referencable: index it onto its fns node.
                node = self.fns_node(m)
                self.index.setdefault(tname, []).append(
                    (m.crate, m.path, self.group_of(m), node.id))
            elif t in ("const_item", "static_item"):
                vis = visibility_of(ch)
                if vis is False:
                    continue
                m.pub_items += 1
                cname = txt(ch.child_by_field_name("name"))
                cty = ch.child_by_field_name("type")
                self.member(self.fns_node(m), cname,
                            f"const {cname}: {collapse(txt(cty))}", "const", vis,
                            type_refs([cty]))

    def extract_impls(self, m: Mod) -> None:
        for ch in m.body.children:
            if ch.type != "impl_item":
                continue
            ty = ch.child_by_field_name("type")
            while ty is not None and ty.type == "generic_type":
                ty = ty.child_by_field_name("type")
            if ty is None or ty.type != "type_identifier":
                continue
            target = self.resolve(txt(ty), m, group=None)
            if target is None:
                continue
            node = self.node_by_id[target]
            tr = ch.child_by_field_name("trait")
            if tr is not None:
                trait_txt = collapse(txt(tr))
                sig = f"impl {trait_txt}"
                if not any(mb.signature == clip(sig) for mb in node.members):
                    self.member(node, f"impl_{trait_txt}", sig, "impl", None, [])
                continue
            body = ch.child_by_field_name("body")
            for it in (body.named_children if body else []):
                if it.type != "function_item":
                    continue
                vis = visibility_of(it)
                if vis is False:
                    continue
                sig, refs = fn_signature(it)
                self.member(node, txt(it.child_by_field_name("name")), sig,
                            "method", vis, type_refs(refs))

    # -- reference resolution ---------------------------------------------------

    def resolve(self, name: str, m: Mod, group: tuple | None) -> str | None:
        """Resolve a type name from module `m` to a node id, or None.

        Ladder: same module -> same group -> the module's use imports ->
        unique across the workspace -> drop (never guess)."""
        cands = self.index.get(name, [])
        if not cands:
            return None
        same_mod = [c for c in cands if c[0] == m.crate and c[1] == m.path]
        if same_mod:
            return same_mod[0][3]
        if group is not None:
            same_group = [c for c in cands if c[2] == group]
            if len(same_group) == 1:
                return same_group[0][3]
            if len(same_group) > 1:
                return None
        imp = m.imports.get(name)
        if imp is not None:
            # `imp` is the full imported path; its defining module is imp[:-1].
            via = [c for c in cands if (c[0], *c[1]) == tuple(imp[:-1])]
            if len(via) == 1:
                return via[0][3]
        if len(cands) == 1:
            return cands[0][3]
        return None

    def resolve_deps(self, mods_by_key: dict[tuple[str, tuple[str, ...]], Mod]) -> None:
        for key, nodes in self.groups.items():
            for node in nodes:
                m = mods_by_key.get((node.crate, node.mod_path))
                if m is None:
                    continue
                for mb in node.members:
                    for ref in mb.ref_names:
                        if ref == node.name:
                            continue
                        target = self.resolve(ref, m, key)
                        if target is None or target == node.id:
                            continue
                        tnode = self.node_by_id[target]
                        # Same code_item only: each item draws its own diagram.
                        if tnode.group != key:
                            continue
                        if target not in mb.deps:
                            mb.deps.append(target)


# --------------------------------------------------------------------------
# Caps
# --------------------------------------------------------------------------


def apply_caps(ex: Extractor) -> None:
    for key in ex.group_order:
        nodes = ex.groups[key]
        for node in nodes:
            if len(node.members) > MAX_ROWS_PER_NODE:
                extra = len(node.members) - (MAX_ROWS_PER_NODE - 1)
                node.members = node.members[: MAX_ROWS_PER_NODE - 1]
                node.members.append(
                    Member(id=ex.fresh_id(f"m_{node.id}_more"),
                           signature=f"… +{extra} more", kind="overflow",
                           visibility=None)
                )
        if len(nodes) > MAX_NODES_PER_ITEM:
            extra = len(nodes) - (MAX_NODES_PER_ITEM - 1)
            kept = nodes[: MAX_NODES_PER_ITEM - 1]
            more = TypeNode(
                id=ex.fresh_id(f"n_api_{ex.group_slug(key)}_more"),
                name=f"… +{extra} more", kind="overflow",
                crate=kept[0].crate, mod_path=(),
                summary=f"{extra} further public items — see the source.",
            )
            ex.groups[key] = kept + [more]
        # Drop deps whose target got capped away.
        alive = {n.id for n in ex.groups[key]}
        for node in ex.groups[key]:
            node.deps = [d for d in node.deps if d in alive]
            for mb in node.members:
                mb.deps = [d for d in mb.deps if d in alive]


# --------------------------------------------------------------------------
# Tier 1 — per-crate module graphs
# --------------------------------------------------------------------------


@dataclass
class CrateGraph:
    crate: str
    # top module name -> (files, pub_items, deps set)
    tops: dict[str, tuple[set, int, set]] = field(default_factory=dict)
    # used workspace crate -> imported names
    externs: dict[str, list[str]] = field(default_factory=dict)
    # top module -> extern crates it uses
    extern_edges: dict[str, set] = field(default_factory=dict)


def build_crate_graph(crate: str, mods: list[Mod]) -> CrateGraph:
    g = CrateGraph(crate=crate)
    for m in mods:
        if not m.path:
            continue  # the root is the facade; its re-exports aren't edges
        top = m.path[0]
        files, items, deps = g.tops.setdefault(top, (set(), 0, set()))
        files.add(m.file)
        g.tops[top] = (files, items + m.pub_items, deps)
        for u in m.uses:
            if u[0] == crate and len(u) >= 2:
                if u[1] != top and u[1] in {mm.path[0] for mm in mods if mm.path}:
                    deps.add(u[1])
            elif u[0] in CRATE_TO_CONTAINER and u[0] != crate:
                g.extern_edges.setdefault(top, set()).add(u[0])
                names = g.externs.setdefault(u[0], [])
                if u[-1] not in names and u[-1][:1].isupper():
                    names.append(u[-1])
    return g


# --------------------------------------------------------------------------
# Emission
# --------------------------------------------------------------------------


def emit_member(lines: list[str], mb: Member, indent: str) -> None:
    lines.append(f"{indent}code_member {mb.id} {{")
    lines.append(f"{indent}  signature = {wcl_str(mb.signature)}")
    lines.append(f"{indent}  kind = {wcl_str(mb.kind)}")
    if mb.visibility:
        lines.append(f"{indent}  visibility = {wcl_str(mb.visibility)}")
    if mb.deps:
        lines.append(f"{indent}  deps = [{', '.join(mb.deps)}]")
    lines.append(f"{indent}}}")


def emit_node(lines: list[str], node: TypeNode) -> None:
    lines.append(f"  code_node {node.id} {{")
    lines.append(f"    name = {wcl_str(node.name)}")
    if node.kind:
        lines.append(f"    kind = {wcl_str(node.kind)}")
    if node.summary:
        lines.append(f"    summary = {wcl_str(node.summary)}")
    if node.deps:
        lines.append(f"    deps = [{', '.join(sorted(node.deps))}]")
    for mb in node.members:
        emit_member(lines, mb, "    ")
    lines.append("  }")


def emit_module_graph(lines: list[str], g: CrateGraph) -> None:
    if not g.tops:
        return
    lines.append(f"code_item code_crate_{g.crate} {{")
    lines.append(f"  container = {CRATE_TO_CONTAINER[g.crate]}")
    lines.append('  name      = "Module graph"')
    lines.append(
        f"  summary   = {wcl_str('Top-level modules, their use-dependencies, and the workspace crates they import (tree-sitter over crates/' + g.crate + '/src).')}"
    )
    lines.append("  kind      = :module_graph")
    stub_ids = {}
    for top, (files, items, deps) in sorted(g.tops.items()):
        nid = f"n_cg_{g.crate}_{slug(top)}"
        dep_ids = [f"n_cg_{g.crate}_{slug(d)}" for d in sorted(deps)]
        for xc in sorted(g.extern_edges.get(top, ())):
            sid = f"n_cg_{g.crate}_xc_{slug(xc)}"
            stub_ids[xc] = sid
            dep_ids.append(sid)
        lines.append(f"  code_node {nid} {{")
        lines.append(f"    name = {wcl_str(top)}")
        lines.append('    kind = "module"')
        fcount = len(files)
        plural = "s" if fcount != 1 else ""
        lines.append(f"    summary = {wcl_str(f'{fcount} file{plural}, {items} pub items')}")
        if dep_ids:
            lines.append(f"    deps = [{', '.join(dep_ids)}]")
        lines.append("  }")
    for xc, sid in sorted(stub_ids.items()):
        names = g.externs.get(xc, [])
        shown = ", ".join(names[:4]) + (f", +{len(names) - 4} more" if len(names) > 4 else "")
        lines.append(f"  code_node {sid} {{")
        lines.append(f"    name = {wcl_str(f'{xc} (crate)')}")
        lines.append('    kind = "extern_crate"')
        if shown:
            lines.append(f"    summary = {wcl_str('uses ' + shown)}")
        lines.append("  }")
    lines += ["}", ""]


def emit_module_api(lines: list[str], ex: Extractor) -> None:
    for key in ex.group_order:
        nodes = ex.groups[key]
        if not any(n.members for n in nodes):
            continue
        if key[0] == "comp":
            item_id = f"code_api_{key[1]}"
            owner = f"  component = {key[1]}"
            name = "Public surface"
        else:
            item_id = f"code_api_{key[1]}_{slug(key[2])}"
            owner = f"  container = {CRATE_TO_CONTAINER[key[1]]}"
            name = f"Public surface — {key[2]}"
        mods = sorted({n.mod_path for n in nodes if n.kind != "overflow"})
        scope = "::".join((nodes[0].crate,) + mods[0]) if len(mods) == 1 else nodes[0].crate
        lines.append(f"code_item {item_id} {{")
        lines.append(owner)
        lines.append(f"  name      = {wcl_str(name)}")
        lines.append(
            f"  summary   = {wcl_str('Pub / pub(crate) types, traits, and functions — the surface a sibling component sees (tree-sitter over ' + scope + ').')}"
        )
        lines.append("  kind      = :module_api")
        for node in nodes:
            emit_node(lines, node)
        lines += ["}", ""]


def emit_stdlib(lines: list[str]) -> None:
    nodes = []
    for path in sorted(REPO_ROOT.glob(STDLIB_GLOB)):
        kinds = WCL_BLOCK.findall(path.read_text())
        if kinds:
            nodes.append((path.stem, sorted(set(kinds))))
    if not nodes:
        return
    lines.append(f"code_item code_mod_{STDLIB_COMPONENT} {{")
    lines.append(f"  component = {STDLIB_COMPONENT}")
    lines.append('  name      = "Block vocabulary"')
    lines.append(
        f"  summary   = {wcl_str('The block kinds each stdlib part declares — the stdlib public surface is its vocabulary.')}"
    )
    lines.append("  kind      = :class_diagram")
    for node_name, members in nodes:
        lines.append(f"  code_node n_{STDLIB_COMPONENT}_{slug(node_name)} {{")
        lines.append(f"    name = {wcl_str(node_name)}")
        lines.append("    members = [")
        lines += [f"      {wcl_str(k)}," for k in members]
        lines.append("    ]")
        lines.append("  }")
    lines += ["}", ""]


# --------------------------------------------------------------------------


def main() -> int:
    lines = [
        "// GENERATED by scripts/extract_modules.py — do not hand-edit; re-run `just wad-extract`.",
        "// Source: tree-sitter-rust over crates/*/src (module graphs + public surfaces);",
        "// @block kinds per wdoc stdlib part.",
        "namespace wcl.wad",
        "",
    ]

    ex = Extractor()
    mods_by_key: dict[tuple[str, tuple[str, ...]], Mod] = {}
    graphs: list[CrateGraph] = []
    all_mods: dict[str, list[Mod]] = {}

    for crate, root in crate_roots():
        mods = walk_crate(crate, root)
        all_mods[crate] = mods
        for m in mods:
            collect_uses(m)
            mods_by_key[(m.crate, m.path)] = m

    # Two passes: types first (so impls in other modules can attach), then impls.
    for mods in all_mods.values():
        for m in mods:
            ex.extract_module(m)
    for mods in all_mods.values():
        for m in mods:
            ex.extract_impls(m)

    ex.resolve_deps(mods_by_key)
    apply_caps(ex)

    for crate, mods in all_mods.items():
        graphs.append(build_crate_graph(crate, mods))

    for g in graphs:
        emit_module_graph(lines, g)
    emit_module_api(lines, ex)
    emit_stdlib(lines)

    OUT.write_text("\n".join(lines))
    n_items = sum(1 for ln in lines if ln.startswith("code_item "))
    print(f"wrote {OUT.relative_to(WAD_ROOT)} ({n_items} code_items)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
