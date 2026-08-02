#!/usr/bin/env python3
"""
PROTOTYPE — THROWAWAY. Ticket 02, wdoc-substrate map.

Question: how is an HTML template authored?

This is a spike of MODEL A — an external HTML file with an expression
language (Jinja-shaped), consuming the *authored block tree* that ticket 01
decided a template receives.

It exists to make two claims checkable rather than arguable:

  1. A slot / field typo is caught BEFORE rendering, with a line number.
     (Today `wdoc_region(c, "heor")` silently returns "".)
  2. The model survives the hard templates, not just the marketing page:
     book (recursive sidebar tree, on-this-page rail, prev/next) and
     presentation (whole deck rendered at once).

Run:  python3 run.py
No deps. Writes rendered HTML to out/ and prints the check report.
"""

import html
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# ─────────────────────────────────────────────────────────────────────
# Fixture: what ticket 01 said a template receives.
#
#   page.blocks  — the AUTHORED block tree (not lowered HTML)
#   page.*       — page-local, derivable from that tree
#   site.*       — site-level, computed once per site and supplied
#   slot.*       — named regions, as block handles
# ─────────────────────────────────────────────────────────────────────


class Block:
    """An authored wdoc block, as a template sees it."""

    def __init__(self, kind, fields=None, children=None, text=None):
        self.kind = kind
        self.fields = fields or {}
        self.children = children or []
        self.text = text or ""

    # The renderer resolves a placed handle. In the real system this is
    # the typed node that replaces the U+FFF9 sentinel hack; here it just
    # emits plausible HTML so the output is inspectable.
    def render(self):
        k = self.kind
        if k in ("h1", "h2", "h3"):
            return '<%s id="%s">%s</%s>' % (
                k, slug(self.text), html.escape(self.text), k)
        if k == "p":
            return "<p>%s</p>" % html.escape(self.text)
        if k == "callout":
            return '<div class="wdoc-callout %s"><p>%s</p></div>' % (
                self.fields.get("kind", "note"), html.escape(self.text))
        if k == "code":
            return '<pre class="wdoc-code" data-lang="%s"><code>%s</code></pre>' % (
                self.fields.get("lang", ""), html.escape(self.text))
        inner = "".join(c.render() for c in self.children)
        return '<div class="wdoc-%s">%s%s</div>' % (
            k, html.escape(self.text), inner)


class Handle:
    """A list of blocks placed by a template. Renders on resolve, not on eval."""

    def __init__(self, blocks):
        self.blocks = list(blocks)

    def __bool__(self):
        return bool(self.blocks)

    def __iter__(self):
        return iter(self.blocks)

    def __len__(self):
        return len(self.blocks)

    def render(self):
        return "\n".join(b.render() for b in self.blocks)


def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


class Rec(dict):
    """A record with attribute access, so templates read `p.href`."""

    def __getattr__(self, k):
        try:
            return self[k]
        except KeyError:
            raise AttributeError(k)


def toc(title, href="", current=False, children=()):
    return Rec(title=title, href=href, current=current,
               children=list(children),
               # `active` is SUPPLIED, not computed in the template — see
               # FINDING 3 in README.md.
               active=current or any(c["active"] for c in children))


PAGE_BLOCKS = [
    Block("h1", text="Sites"),
    Block("p", text="A site gathers pages and renders them through a template."),
    Block("callout", {"kind": "note"}, text="A page not in the toc is still built."),
    Block("h2", text="Declaring a site"),
    Block("code", {"lang": "wcl"}, text='site docs {\n  template = :book\n}'),
    Block("h2", text="Templates"),
    Block("p", text="Four ship in the stdlib: webpage, book, website, presentation."),
    Block("h3", text="Regions"),
    Block("p", text="A region renders independently of the page body."),
]

SITE = Rec(
    title="WCL",
    home_href="",
    home_title="WCL",
    theme_toggle=True,
    search=True,
    pages=[Rec(name="index", href="index.html"),
           Rec(name="sites", href="sites.html"),
           Rec(name="pages", href="pages.html")],
    menu=[Rec(label="Docs", href="index.html", current=False, children=[]),
          Rec(label="Reference", href="", current=False, children=[
              Rec(label="Sites", href="sites.html", current=True, children=[]),
              Rec(label="Pages", href="pages.html", current=False, children=[])]),
          Rec(label="GitHub", href="https://github.com/wiltaylor/wcl",
              current=False, children=[])],
    toc=[toc("Getting started", "index.html"),
         toc("Reference", children=[
             toc("Sites", "sites.html", current=True),
             toc("Pages", "pages.html")]),
         toc("Appendix", children=[toc("Glossary", "glossary.html")])],
    footer=[Rec(label="GitHub", href="https://github.com/wiltaylor/wcl",
                current=False, icon="<svg class='wdoc-icon'></svg>")],
    deck=[Rec(title="Intro", slides=[
              Rec(title="Title", content=Handle([Block("h1", text="wdoc")]),
                  notes=Handle([Block("p", text="Say hello.")])),
              Rec(title="Why", content=Handle([Block("p", text="Because HTML.")]),
                  notes=Handle([]))]),
          Rec(title="Body", slides=[
              Rec(title="Slots", content=Handle([Block("h2", text="Slots")]),
                  notes=Handle([]))])],
)

PAGE = Rec(
    name="sites",
    title="Sites",
    blocks=Handle(PAGE_BLOCKS),
    # Page-local nav, SUPPLIED rather than folded in-template (FINDING 3).
    prev=Rec(title="Getting started", href="index.html"),
    next=Rec(title="Pages", href="pages.html"),
)

REGIONS = {
    "hero": Handle([Block("h1", text="Configuration that checks itself"),
                    Block("p", text="A typed language for documents.")]),
    "footer": Handle([Block("p", text="MIT licensed.")]),
}

# ─────────────────────────────────────────────────────────────────────
# Types, for the check pass. This is what makes a typo an error instead
# of an empty string.
# ─────────────────────────────────────────────────────────────────────

T_PAGEREF = {"name": "utf8", "href": "utf8"}
T_TOC = {"title": "utf8", "href": "utf8", "current": "bool",
         "active": "bool", "children": "list<TocEntry>"}
T_MENU = {"label": "utf8", "href": "utf8", "current": "bool",
          "children": "list<MenuEntry>"}
T_FOOTER = {"label": "utf8", "href": "utf8", "current": "bool", "icon": "utf8"}
T_BLOCK = {"kind": "utf8", "text": "utf8", "fields": "map", "children": "blocks"}
T_SLIDE = {"title": "utf8", "content": "blocks", "notes": "blocks"}
T_SECTION = {"title": "utf8", "slides": "list<DeckSlide>"}
T_NAV = {"title": "utf8", "href": "utf8"}

TYPES = {
    "PageRef": T_PAGEREF, "TocEntry": T_TOC, "MenuEntry": T_MENU,
    "FooterButton": T_FOOTER, "Block": T_BLOCK, "DeckSlide": T_SLIDE,
    "DeckSection": T_SECTION, "PageNav": T_NAV,
}

SITE_T = {
    "title": "utf8", "home_href": "utf8", "home_title": "utf8",
    "theme_toggle": "bool", "search": "bool",
    "pages": "list<PageRef>", "menu": "list<MenuEntry>",
    "toc": "list<TocEntry>", "footer": "list<FooterButton>",
    "deck": "list<DeckSection>",
}
PAGE_T = {
    "name": "utf8", "title": "utf8", "blocks": "blocks",
    "prev": "PageNav", "next": "PageNav",
}

ELEM = re.compile(r"^list<(\w+)>$")

FILTERS = {
    # name: (arity, result type)
    "text": (0, "utf8"),
    "first_of": (1, "blocks"),
    "where": (1, "blocks"),
    "default": (1, "utf8"),
    "count": (0, "i64"),
}

# ─────────────────────────────────────────────────────────────────────
# Template language — parse
# ─────────────────────────────────────────────────────────────────────

TAG = re.compile(r"\{\{(.*?)\}\}|\{%(.*?)%\}|\{#(.*?)#\}", re.S)


class Node:
    def __init__(self, kind, line, **kw):
        self.kind = kind
        self.line = line
        self.__dict__.update(kw)


def tokenize(src):
    pos, line, out = 0, 1, []
    for m in TAG.finditer(src):
        if m.start() > pos:
            chunk = src[pos:m.start()]
            out.append(("text", chunk, line))
            line += chunk.count("\n")
        body = m.group(1) or m.group(2) or m.group(3) or ""
        kind = "out" if m.group(1) is not None else (
            "stmt" if m.group(2) is not None else "comment")
        out.append((kind, body.strip(), line))
        line += m.group(0).count("\n")
        pos = m.end()
    if pos < len(src):
        out.append(("text", src[pos:], line))
    return out


class ParseError(Exception):
    pass


def parse(src, name):
    toks = tokenize(src)
    i = 0
    slots = {}

    def block(stop=()):
        nonlocal i
        body = []
        while i < len(toks):
            kind, text, line = toks[i]
            if kind == "comment":
                i += 1
                continue
            if kind == "text":
                body.append(Node("text", line, text=text))
                i += 1
                continue
            if kind == "out":
                body.append(Node("out", line, expr=parse_expr(text, name, line)))
                i += 1
                continue
            head = text.split(None, 1)[0] if text else ""
            if head in stop:
                return body
            i += 1
            if head == "if":
                cond = parse_expr(text[2:].strip(), name, line)
                then = block(("else", "elif", "endif"))
                alts, otherwise = [], []
                while toks[i][1].split(None, 1)[0] == "elif":
                    eline = toks[i][2]
                    econd = parse_expr(toks[i][1][4:].strip(), name, eline)
                    i += 1
                    alts.append((econd, block(("else", "elif", "endif"))))
                if toks[i][1].strip() == "else":
                    i += 1
                    otherwise = block(("endif",))
                i += 1  # endif
                body.append(Node("if", line, cond=cond, then=then,
                                 alts=alts, otherwise=otherwise))
            elif head == "for":
                m = re.match(r"for\s+(\w+)\s+in\s+(.+)$", text, re.S)
                if not m:
                    raise ParseError("%s:%d: malformed for" % (name, line))
                var, seq = m.group(1), parse_expr(m.group(2), name, line)
                inner = block(("endfor",))
                i += 1
                body.append(Node("for", line, var=var, seq=seq, body=inner))
            elif head == "macro":
                m = re.match(r"macro\s+(\w+)\s*\((.*?)\)$", text, re.S)
                if not m:
                    raise ParseError("%s:%d: malformed macro" % (name, line))
                params = [p.strip() for p in m.group(2).split(",") if p.strip()]
                inner = block(("endmacro",))
                i += 1
                body.append(Node("macro", line, name=m.group(1),
                                 params=params, body=inner))
            elif head == "slots":
                decl = block(("endslots",))
                i += 1
                raw = "".join(n.text for n in decl if n.kind == "text")
                for ln in raw.splitlines():
                    ln = ln.split("#")[0].strip()
                    if not ln:
                        continue
                    dm = re.match(r"(\w+)(\??)\s*:\s*(\w+)$", ln)
                    if not dm:
                        raise ParseError(
                            "%s: malformed slot declaration %r" % (name, ln))
                    slots[dm.group(1)] = Rec(optional=dm.group(2) == "?",
                                             type=dm.group(3))
            else:
                raise ParseError("%s:%d: unknown tag %r" % (name, line, head))
        return body

    body = block()
    return Node("template", 1, name=name, body=body, slots=slots)


# ── expressions ──────────────────────────────────────────────────────

TOKEN = re.compile(r"""
    \s*(?:
      (?P<str>"[^"]*"|'[^']*')
    | (?P<num>\d+)
    | (?P<op>\|\||&&|==|!=|\(|\)|,|\.|\|)
    | (?P<word>[A-Za-z_]\w*)
    )""", re.X)

KEYWORDS = {"or": "||", "and": "&&", "not": "!"}


def lex_expr(s, name, line):
    out, pos = [], 0
    while pos < len(s):
        m = TOKEN.match(s, pos)
        if not m:
            if s[pos:].strip() == "":
                break
            raise ParseError("%s:%d: bad expression near %r"
                             % (name, line, s[pos:pos + 12]))
        pos = m.end()
        if m.group("word") and m.group("word") in KEYWORDS:
            out.append(("op", KEYWORDS[m.group("word")]))
        else:
            for g in ("str", "num", "op", "word"):
                if m.group(g):
                    out.append((g, m.group(g)))
                    break
    return out


def parse_expr(s, name, line):
    toks = lex_expr(s, name, line)
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else (None, None)

    def eat(val=None):
        nonlocal pos
        t = toks[pos]
        pos += 1
        return t

    def primary():
        nonlocal pos
        kind, val = peek()
        if kind == "str":
            eat()
            return Node("lit", line, value=val[1:-1], type="utf8")
        if kind == "num":
            eat()
            return Node("lit", line, value=int(val), type="i64")
        if kind == "op" and val == "(":
            eat()
            e = expr()
            eat()  # )
            return e
        if kind == "word":
            eat()
            node = Node("name", line, name=val)
            return postfix(node)
        raise ParseError("%s:%d: unexpected %r in expression"
                         % (name, line, val))

    def postfix(node):
        nonlocal pos
        while True:
            kind, val = peek()
            if kind == "op" and val == ".":
                eat()
                k2, field = peek()
                if k2 != "word":
                    raise ParseError("%s:%d: expected field name" % (name, line))
                eat()
                node = Node("field", line, target=node, field=field)
            elif kind == "op" and val == "(":
                eat()
                args = []
                if peek()[1] != ")":
                    args.append(expr())
                    while peek()[1] == ",":
                        eat()
                        args.append(expr())
                eat()  # )
                node = Node("call", line, target=node, args=args)
            elif kind == "op" and val == "|":
                eat()
                k2, fname = peek()
                if k2 != "word":
                    raise ParseError("%s:%d: expected filter name" % (name, line))
                eat()
                args = []
                if peek()[1] == "(":
                    eat()
                    if peek()[1] != ")":
                        args.append(expr())
                        while peek()[1] == ",":
                            eat()
                            args.append(expr())
                    eat()
                node = Node("filter", line, target=node, name=fname, args=args)
            else:
                return node

    def unary():
        kind, val = peek()
        if kind == "op" and val == "!":
            eat()
            return Node("not", line, target=unary())
        return primary()

    def expr():
        left = unary()
        while True:
            kind, val = peek()
            if kind == "op" and val in ("||", "&&", "==", "!="):
                eat()
                right = unary()
                left = Node("bin", line, op=val, left=left, right=right)
            else:
                return left

    e = expr()
    return e


# ─────────────────────────────────────────────────────────────────────
# CHECK PASS — the point of the whole spike
# ─────────────────────────────────────────────────────────────────────

class Checker:
    def __init__(self, tmpl):
        self.t = tmpl
        self.errors = []
        self.macros = {}

    def err(self, line, msg):
        self.errors.append("%s:%d: %s" % (self.t.name, line, msg))

    def run(self):
        self.collect_macros(self.t.body)
        scope = {"site": "Site", "page": "Page", "slot": "Slots"}
        self.walk(self.t.body, scope)
        return self.errors

    def collect_macros(self, body):
        for n in body:
            if n.kind == "macro":
                self.macros[n.name] = n
            for attr in ("then", "otherwise", "body"):
                if hasattr(n, attr):
                    self.collect_macros(getattr(n, attr))
            for _, b in getattr(n, "alts", []):
                self.collect_macros(b)

    def walk(self, body, scope):
        for n in body:
            if n.kind == "out":
                self.type_of(n.expr, scope)
            elif n.kind == "if":
                self.type_of(n.cond, scope)
                self.walk(n.then, scope)
                for c, b in n.alts:
                    self.type_of(c, scope)
                    self.walk(b, scope)
                self.walk(n.otherwise, scope)
            elif n.kind == "for":
                t = self.type_of(n.seq, scope)
                m = ELEM.match(t or "")
                if t == "blocks":
                    el = "Block"
                elif m:
                    el = m.group(1)
                elif t in ("?", None):
                    el = "?"
                else:
                    self.err(n.line, "`for` over %s, which is not a list" % t)
                    el = "?"
                inner = dict(scope)
                inner[n.var] = el
                self.walk(n.body, inner)
            elif n.kind == "macro":
                inner = dict(scope)
                for p in n.params:
                    inner[p] = "?"       # macro params are untyped in this spike
                self.walk(n.body, inner)

    def type_of(self, e, scope):
        k = e.kind
        if k == "lit":
            return e.type
        if k == "not":
            self.type_of(e.target, scope)
            return "bool"
        if k == "bin":
            self.type_of(e.left, scope)
            self.type_of(e.right, scope)
            return "bool"
        if k == "name":
            if e.name in scope:
                return scope[e.name]
            if e.name in self.macros:
                return "macro"
            self.err(e.line, "unknown name `%s` — in scope: %s"
                     % (e.name, ", ".join(sorted(scope))))
            return "?"
        if k == "call":
            for a in e.args:
                self.type_of(a, scope)
            t = self.type_of(e.target, scope)
            if t == "macro":
                mac = self.macros[e.target.name]
                if len(e.args) != len(mac.params):
                    self.err(e.line, "macro `%s` takes %d argument(s), given %d"
                             % (e.target.name, len(mac.params), len(e.args)))
                return "utf8"
            if t != "?":
                self.err(e.line, "`%s` is not callable" % self.render_path(e.target))
            return "?"
        if k == "filter":
            t = self.type_of(e.target, scope)
            for a in e.args:
                self.type_of(a, scope)
            spec = FILTERS.get(e.name)
            if not spec:
                self.err(e.line, "unknown filter `%s` — known: %s"
                         % (e.name, ", ".join(sorted(FILTERS))))
                return "?"
            arity, res = spec
            if len(e.args) != arity:
                self.err(e.line, "filter `%s` takes %d argument(s), given %d"
                         % (e.name, arity, len(e.args)))
            return res
        if k == "field":
            t = self.type_of(e.target, scope)
            if t == "Slots":
                decl = self.t.slots.get(e.field)
                if not decl:
                    self.err(e.line,
                             "unknown slot `%s` — declared: %s"
                             % (e.field, ", ".join(sorted(self.t.slots)) or "(none)"))
                    return "?"
                return decl.type
            fields = ({"Site": SITE_T, "Page": PAGE_T}.get(t)
                      or TYPES.get(t or ""))
            if fields is None:
                if t not in ("?", None):
                    self.err(e.line, "`%s` is a %s — it has no field `%s`"
                             % (self.render_path(e.target), t, e.field))
                return "?"
            if e.field not in fields:
                self.err(e.line, "`%s` has no field `%s` — has: %s"
                         % (self.render_path(e.target), e.field,
                            ", ".join(sorted(fields))))
                return "?"
            return fields[e.field]
        return "?"

    def render_path(self, e):
        if e.kind == "name":
            return e.name
        if e.kind == "field":
            return "%s.%s" % (self.render_path(e.target), e.field)
        return "<expr>"


# ─────────────────────────────────────────────────────────────────────
# Render
# ─────────────────────────────────────────────────────────────────────

class Renderer:
    def __init__(self, tmpl, slots):
        self.t = tmpl
        self.macros = {}
        self.slots = Rec({k: slots.get(k, Handle([])) for k in tmpl.slots})
        Checker(tmpl).collect_macros.__call__  # noqa - keep symmetry
        self._collect(tmpl.body)

    def _collect(self, body):
        for n in body:
            if n.kind == "macro":
                self.macros[n.name] = n
            for attr in ("then", "otherwise", "body"):
                if hasattr(n, attr):
                    self._collect(getattr(n, attr))
            for _, b in getattr(n, "alts", []):
                self._collect(b)

    def render(self):
        env = {"site": SITE, "page": PAGE, "slot": self.slots}
        return self.exec(self.t.body, env)

    def exec(self, body, env):
        out = []
        for n in body:
            if n.kind == "text":
                out.append(n.text)
            elif n.kind == "out":
                out.append(self.emit(self.eval(n.expr, env)))
            elif n.kind == "if":
                if truthy(self.eval(n.cond, env)):
                    out.append(self.exec(n.then, env))
                else:
                    for c, b in n.alts:
                        if truthy(self.eval(c, env)):
                            out.append(self.exec(b, env))
                            break
                    else:
                        out.append(self.exec(n.otherwise, env))
            elif n.kind == "for":
                seq = self.eval(n.seq, env) or []
                for item in seq:
                    inner = dict(env)
                    inner[n.var] = item
                    out.append(self.exec(n.body, inner))
            elif n.kind == "macro":
                pass
        return "".join(out)

    def emit(self, v):
        if isinstance(v, Handle):
            return v.render()          # a placed handle resolves here
        if isinstance(v, Block):
            return v.render()
        if isinstance(v, bool):
            return "true" if v else ""
        if v is None:
            return ""
        return str(v)

    def eval(self, e, env):
        k = e.kind
        if k == "lit":
            return e.value
        if k == "name":
            if e.name in env:
                return env[e.name]
            if e.name in self.macros:
                return ("macro", e.name)
            return None
        if k == "not":
            return not truthy(self.eval(e.target, env))
        if k == "bin":
            l = self.eval(e.left, env)
            if e.op == "||":
                return l if truthy(l) else self.eval(e.right, env)
            r = self.eval(e.right, env)
            if e.op == "&&":
                return r if truthy(l) else l
            if e.op == "==":
                return l == r
            return l != r
        if k == "field":
            tgt = self.eval(e.target, env)
            if tgt is None:
                return None
            if isinstance(tgt, (dict, Rec)):
                return tgt.get(e.field)
            return getattr(tgt, e.field, None)
        if k == "call":
            v = self.eval(e.target, env)
            if isinstance(v, tuple) and v[0] == "macro":
                mac = self.macros[v[1]]
                inner = dict(env)
                for p, a in zip(mac.params, e.args):
                    inner[p] = self.eval(a, env)
                return self.exec(mac.body, inner)
            return None
        if k == "filter":
            v = self.eval(e.target, env)
            args = [self.eval(a, env) for a in e.args]
            return self.apply_filter(e.name, v, args)
        return None

    def apply_filter(self, name, v, args):
        if name == "text":
            if isinstance(v, Handle):
                return " ".join(b.text for b in v)
            if isinstance(v, Block):
                return v.text
            return "" if v is None else str(v)
        if name == "first_of":
            blocks = list(v) if v is not None else []
            hit = [b for b in blocks if b.kind == args[0]][:1]
            return Handle(hit)
        if name == "where":
            blocks = list(v) if v is not None else []
            return Handle([b for b in blocks if b.kind == args[0]])
        if name == "default":
            return v if truthy(v) else args[0]
        if name == "count":
            return len(v) if v is not None else 0
        return None


def truthy(v):
    if isinstance(v, Handle):
        return bool(v.blocks)
    return bool(v)


# ─────────────────────────────────────────────────────────────────────
# Driver
# ─────────────────────────────────────────────────────────────────────

CASES = [
    ("website.html", "The docs website — header, hero, content, sidebar, footer",
     REGIONS, True),
    ("book.html", "The book — recursive sidebar tree, rail, prev/next",
     {}, True),
    ("presentation.html", "The deck — whole site rendered at once", {}, True),
    ("website-typo.html", "DELIBERATELY BROKEN — the check pass must catch it",
     REGIONS, False),
]


def main():
    ok = True
    for fname, blurb, slots, should_pass in CASES:
        path = os.path.join(HERE, "model-a-html", fname)
        src = open(path).read()
        print("\n\033[1m── %s\033[0m — %s" % (fname, blurb))
        try:
            tmpl = parse(src, fname)
        except ParseError as exc:
            print("  parse error: %s" % exc)
            ok = False
            continue
        print("  slots declared: %s" % (", ".join(
            "%s%s: %s" % (k, "?" if v.optional else "", v.type)
            for k, v in tmpl.slots.items()) or "(none)"))
        errors = Checker(tmpl).run()
        if errors:
            print("  \033[31mCHECK FAILED\033[0m (%d)" % len(errors))
            for e in errors:
                print("    %s" % e)
            if should_pass:
                ok = False
            continue
        print("  \033[32mcheck passed\033[0m")
        if not should_pass:
            print("  \033[31m!! expected this to fail and it did not\033[0m")
            ok = False
            continue
        # Unsupplied required slot → also an error, at render time.
        missing = [k for k, v in tmpl.slots.items()
                   if not v.optional and k not in slots and k != "content"]
        if missing:
            print("  \033[31mmissing required slot(s): %s\033[0m"
                  % ", ".join(missing))
            ok = False
            continue
        supplied = dict(slots)
        supplied.setdefault("content", PAGE.blocks)
        out = Renderer(tmpl, supplied).render()
        dest = os.path.join(HERE, "out", fname)
        open(dest, "w").write(out)
        print("  rendered %d bytes → out/%s" % (len(out), fname))
    print()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
