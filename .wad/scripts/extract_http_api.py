#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Extract the wdoc dev server's public HTTP surface from the axum route
registrations in crates/wcl/src/serve.rs into data/generated/http_api.wcl:
one :api code_item whose api_endpoint rows are the real routes. The route
list is mechanical; the swagger-level detail (params / request / responses)
lives in the PATH_DETAILS table below — extend it when a handler's contract
changes (the shapes come from edit.rs / serve.rs handler code)."""

import re
import sys
from pathlib import Path

WAD_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = WAD_ROOT.parent
SRC = REPO_ROOT / "crates" / "wcl" / "src" / "serve.rs"
OUT = WAD_ROOT / "data" / "generated" / "http_api.wcl"

# Route path -> human summary. Extend THIS table (never the generated file)
# when a new endpoint lands; unmapped routes get a placeholder that says so.
PATH_SUMMARIES = {
    "/__wdoc_reload": "Live-reload long-poll: the injected client waits here and reloads the page when a rebuild lands.",
    "/__wdoc_rebuild": "Trigger a rebuild; scoped to the sending page's sub-site, blocks until the build finishes.",
    "/__wdoc_comment.js": "The review-comment client script (comment mode).",
    "/__wdoc_comment": "Create / list review comments in the comments.wcl sidecar.",
    "/__wdoc_review/status": "Long-poll the review handshake (agent-waiting marker).",
    "/__wdoc_review/ready": "Reviewer clicked “Send to agent” — release the blocked `wcl wdoc review`.",
    "/__wdoc_edit.js": "The WYSIWYG edit client script (edit mode).",
    "/__wdoc_editor.js": "The shared source-editor component (syntect-highlighted textarea).",
    "/__wdoc_objects": "Schema data objects for the edit panel, filtered by the page's namespace.",
    "/__wdoc_edit/add": "Insert a new block (WYSIWYG structure edit).",
    "/__wdoc_edit/delete": "Delete a block (WYSIWYG structure edit).",
    "/__wdoc_edit/field": "Rewrite one block field through the `wcl set` pipeline.",
    "/__wdoc_edit/move": "Reorder a block among its siblings.",
    "/__wdoc_object": "One schema data object's instance list for the object editor.",
    "/__wdoc_object_kinds": "The @block/@table kinds the object browser offers, namespace-filtered.",
    "/__wdoc_object_source": "An instance's raw WCL source for the text editor (byte-spliced back on save).",
    "/__wdoc_object_template": "A fresh-instance template for a schema kind (new-object flow).",
    "/__wdoc_schema": "The document's schema description backing the edit panel forms.",
    "/__wdoc_edit": "Apply a block/field/structure edit through the validating `wcl set` pipeline.",
    "/__wdoc_check": "Dry-run diagnostics for an unsaved buffer (syntax + schema errors introduced).",
    "/__wdoc_format": "Format a buffer with the `wcl fmt` core.",
    "/__wdoc_files": "File tree for the source editor, scoped to the page's owning sub-site.",
    "/__wdoc_file": "Whole-file source read/save with content-etag conflict detection.",
    "/__wdoc_highlight": "Syntax-highlight a code snippet (public wcl_wdoc::highlight_code).",
    "/__wdoc_preview": "Render the current page with unsaved buffers overlaid into a scratch dir.",
    "/__wdoc_preview/{*path}": "Serve the preview render (no reload/edit scripts injected).",
}

# Swagger-level detail per route: query/body parameters, request/response
# shapes, per-status rows. Keys are optional; anything present becomes the
# endpoint's params / request / responses children. Shapes mirror the
# handlers in crates/wcl/src/{edit.rs,serve.rs}.
JSON = "application/json"
PATH_DETAILS: dict[str, dict] = {
    "/__wdoc_check": {
        "description": "Pass 1 checks the buffer's syntax with exact line/col; pass 2 overlays the buffer on the owning document and reports only the schema errors the edit *introduces* over the on-disk baseline.",
        "request_media_type": JSON,
        "request": '{ "path": "<served-tree .wcl file>", "text": "<buffer>", "page_file": "<current page (optional)>" }',
        "responses": [
            {"status": "200", "media_type": JSON, "description": "Check result — `ok` is true when no diagnostics.",
             "schema": '{ "ok": bool, "diagnostics": [{ "scope": "syntax"|"schema", "message": str, "offset"?: int, "length"?: int, "line"?: int, "col"?: int }] }'},
            {"status": "400", "media_type": JSON, "description": "Bad JSON body, or a path outside the served tree."},
        ],
    },
    "/__wdoc_format": {
        "request_media_type": JSON,
        "request": '{ "text": "<buffer>" }',
        "responses": [
            {"status": "200", "media_type": JSON, "description": "The canonically formatted source.",
             "schema": '{ "text": str }'},
            {"status": "400", "media_type": JSON, "description": "The buffer does not parse (format needs a valid tree)."},
        ],
    },
    "/__wdoc_file": {
        "description": "GET reads; POST saves through the same validate-then-write pipeline every editor write uses. Only `.wcl` files may be saved.",
        "params": [
            {"name": "path", "location": "query", "type": "string", "required": True,
             "description": "File path inside the served tree (GET read)."},
        ],
        "request_media_type": JSON,
        "request": '{ "path": str, "text": str, "base_etag"?: str }',
        "responses": [
            {"status": "200", "media_type": JSON, "description": "Read: the file text plus its content etag. Save: confirmation with the new etag.",
             "schema": 'GET: { "path": str, "text": str, "etag": str }   POST: { "ok": true, "etag": str, "result": str }'},
            {"status": "400", "media_type": JSON, "description": "Save conflict (`base_etag` no longer matches — the file changed on disk), non-.wcl target, or a path outside the served tree."},
        ],
    },
    "/__wdoc_files": {
        "responses": [
            {"status": "200", "media_type": JSON, "description": "The `.wcl` file tree scoped to the current page's owning sub-site."},
        ],
    },
    "/__wdoc_rebuild": {
        "description": "Blocks until the build finishes so the client can show a spinner then a done toast. A request carrying `page_file` rebuilds only that page's owning sub-site; without it the whole served site rebuilds.",
        "request_media_type": JSON,
        "request": '{ "page_file": "<the page the Rebuild button was on (optional)>" }',
        "responses": [
            {"status": "200", "media_type": JSON, "description": "What was rebuilt and how it went.",
             "schema": '{ "ok": bool, "scope": "site" | "<sub-site subdir>", "summary": str }'},
            {"status": "500", "media_type": JSON, "description": "The rebuild worker is gone or the build was cancelled."},
        ],
    },
    "/__wdoc_review/status": {
        "description": "Long-poll: returns when the agent-waiting marker changes, so the toolbar can show/hide the “Send to agent” banner.",
        "responses": [
            {"status": "200", "media_type": JSON, "description": "The current handshake state."},
        ],
    },
    "/__wdoc_review/ready": {
        "responses": [
            {"status": "200", "media_type": JSON, "description": "The blocked `wcl wdoc review` was released.", "schema": '{ "ok": true }'},
            {"status": "400", "media_type": JSON, "description": "No review handshake is active (no agent is waiting)."},
        ],
    },
    "/__wdoc_comment": {
        "description": "Comment-mode only. Creates or lists the review notes persisted in the `comments.wcl` sidecar beside the page's owning document; no rebuild happens.",
        "responses": [
            {"status": "200", "media_type": JSON, "description": "The comment list (GET) or the stored comment (POST)."},
            {"status": "400", "media_type": JSON, "description": "Missing comment body or page on create."},
        ],
    },
    "/__wdoc_preview": {
        "description": "Renders the current page with unsaved buffers overlaid into a scratch TempDir. The first preview warms with a full sub-site build; later calls are targeted single-page renders.",
        "request_media_type": JSON,
        "request": '{ "page_file": str, "buffers": { "<path>": "<unsaved text>", … } }',
        "responses": [
            {"status": "200", "media_type": JSON, "description": "Where the preview iframe should navigate."},
        ],
    },
}


def wcl_str(s: str) -> str:
    out = s.replace("\\", "\\\\").replace('"', '\\"')
    out = out.replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")
    return f'"{out}"'


def slug(s: str) -> str:
    return re.sub(r"_+", "_", "".join(c if c.isalnum() else "_" for c in s)).strip("_")


def main() -> int:
    src = SRC.read_text()
    # `.route("path", get(...))` — the path and method may sit on the next
    # line(s), so match across whitespace.
    routes: dict[str, set[str]] = {}
    for m in re.finditer(
        r"\.route\(\s*\"([^\"]+)\"\s*,\s*(?:axum::routing::)?(get|post|put|delete)\(",
        src,
        re.DOTALL,
    ):
        routes.setdefault(m.group(1), set()).add(m.group(2).upper())

    lines = [
        "// GENERATED by scripts/extract_http_api.py — do not hand-edit; re-run `just wad-extract`.",
        "// Source: axum route registrations in crates/wcl/src/serve.rs",
        "namespace wcl.wad",
        "",
        "code_item serve_http_api {",
        "  component = cli_serve",
        '  name      = "Dev-server HTTP surface"',
        '  summary   = "The endpoints `wcl wdoc serve` exposes (extracted from the axum router; comment/edit-mode routes appear only when those flags are on)."',
        "  kind      = :api",
    ]
    for path in sorted(routes):
        methods = "/".join(sorted(routes[path]))
        summary = PATH_SUMMARIES.get(path, "(describe in extract_http_api.py's PATH_SUMMARIES)")
        detail = PATH_DETAILS.get(path, {})
        eid = slug(path)
        lines += [
            f"  api_endpoint ep_{eid} {{",
            f"    method = {wcl_str(methods)}  path = {wcl_str(path)}",
            f"    summary = {wcl_str(summary)}",
        ]
        if d := detail.get("description"):
            lines.append(f"    description = {wcl_str(d)}")
        if mt := detail.get("request_media_type"):
            lines.append(f"    request_media_type = {wcl_str(mt)}")
        if rq := detail.get("request"):
            lines.append(f"    request = {wcl_str(rq)}")
        for i, p in enumerate(detail.get("params", [])):
            lines += [
                f"    api_param pp_{eid}_{i} {{",
                f"      name = {wcl_str(p['name'])}  location = :{p['location']}",
                *([f"      type = {wcl_str(p['type'])}"] if p.get("type") else []),
                *(["      required = true"] if p.get("required") else []),
                *([f"      description = {wcl_str(p['description'])}"] if p.get("description") else []),
                "    }",
            ]
        for i, r in enumerate(detail.get("responses", [])):
            lines += [
                f"    api_response rr_{eid}_{i} {{",
                f"      status = {wcl_str(r['status'])}",
                f"      description = {wcl_str(r['description'])}",
                *([f"      media_type = {wcl_str(r['media_type'])}"] if r.get("media_type") else []),
                *([f"      schema = {wcl_str(r['schema'])}"] if r.get("schema") else []),
                "    }",
            ]
        lines.append("  }")
    lines += ["}", ""]
    OUT.write_text("\n".join(lines))
    print(f"wrote {OUT.relative_to(WAD_ROOT)} ({len(routes)} endpoints)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
