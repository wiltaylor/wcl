/* Shared helpers for the schema-generated property forms — the diagram
   ShapePanel, the Systems view's NodePanel and detail sections, the add
   dialogs and the Data-mode row all build their rows from a kind's
   `effective_fields` metadata (`/api/palette`'s `diagram_kinds`,
   `/api/systems`'s `kinds`, `/api/data/types`) merged with the instance's
   CELLS, and commit them as block ops.

   Everything here is pure: field ordering, reading a cell, choosing which
   control a (field, cell) pair wants ({@link controlFor}), and turning a
   draft into ops ({@link draftOps}). Rendering that control is
   `components/design/FieldControl.jsx` — a thin switch over `controlFor`,
   with no decisions of its own — and layout stays with each host, because
   a docked panel, a modal section, a dialog and a table row legitimately
   differ.

   The cell shape is the server's one cell type (`editor/cell.rs`): every
   endpoint answers `cells = { labels: [cell], fields: { name: cell } }`,
   each cell `{ state, text, items?, rows? }` with state one of
   `text | identifier | symbol | bool | number | list | rows | computed`. */

/** Geometry fields lead the diagram form, in this order. */
export const GEOMETRY = ['x', 'y', 'width', 'height'];

/** A declared type with the optional marker stripped. */
export const bareType = (field) => (field?.type ?? '').replace(/\?$/, '');

/* Type predicates. These match the WHOLE type name on purpose: a prefix
   test reads `identifier` as an integer and `utf8` as an unsigned one, which
   silently turns a reference into `NaN` and a string field into `0`. */
const INT = /^[iu](8|16|32|64|128|size)$/;
const FLOAT = /^f(32|64)$/;
export const isInt = (ty) => INT.test(ty);
export const isFloat = (ty) => FLOAT.test(ty);
export const isText = (ty) => ty === 'utf8' || ty === 'ascii' || ty.startsWith('utf8<') || ty.startsWith('ascii<');

/** Is this field an inline label slot rather than a named field? */
export const isSlot = (field) =>
  field?.inline_slot !== null && field?.inline_slot !== undefined;

/** Order schema fields for a form: inline slots, geometry, then the rest. */
export function orderFields(fields) {
  const slot = (f) => (isSlot(f) ? 0 : 1);
  const geo = (f) => {
    const i = GEOMETRY.indexOf(f.name);
    return i === -1 ? GEOMETRY.length : i;
  };
  return [...fields].sort(
    (a, b) =>
      slot(a) - slot(b) ||
      (a.inline_slot ?? 0) - (b.inline_slot ?? 0) ||
      geo(a) - geo(b) ||
      a.name.localeCompare(b.name),
  );
}

/** The cell a schema field addresses in a block's cells: labels by
    position, fields by name — no synthetic slot keys to build and parse. */
export const cellOf = (field, cells) =>
  isSlot(field) ? cells?.labels?.[field.inline_slot] : cells?.fields?.[field.name];

/** A named field's cell, for the callers that know the name but have no
    schema field to hand (a summary row, a surface gate). */
export const cellNamed = (cells, name) => cells?.fields?.[name];

/** A named field's text, '' when unset or when it has no single value. */
export const cellText = (cells, name) => cellNamed(cells, name)?.text ?? '';

/** A field's current text ('' when unset). A list cell reads as its
    members on one comma-separated line — the same line every panel edits. */
export function fieldText(field, cells) {
  const cell = cellOf(field, cells);
  if (cell?.state === 'list') return (cell.items ?? []).map((i) => i.text ?? '').join(', ');
  return cell?.text ?? '';
}

/** The states a form control can edit. `computed` (an interpolation, a
    call) and `rows` (a grid) have no single control and hand off to the
    source editor. */
export const formEditable = (state) =>
  ['text', 'identifier', 'symbol', 'bool', 'number', 'list', 'absent'].includes(state);

/**
 * Why a cell has no form control, in the two lengths its hosts want: a
 * compact marker for a table cell and an explanation for a disabled input.
 * One vocabulary, so the same value is not "(expr)" in the Data table and
 * something else in the form that opens over it.
 */
export const NOT_EDITABLE = {
  rows: { short: '(grid)', long: '(a grid — edit as source)' },
  computed: { short: '(expr)', long: '(computed — edit as source)' },
};
export const notEditable = (cell) => NOT_EDITABLE[cell?.state === 'rows' ? 'rows' : 'computed'];

/**
 * How a cell's value is written back: `text` is the ONLY state written as a
 * string literal — every other state round-trips as parsed WCL (see the
 * states in `editor/cell.rs`). {@link valueOp} types a write from the
 * DECLARED type; this is for the structured editors that hold a cell but no
 * schema field, so an `identifier` label (a `code` block's language) is not
 * quietly rewritten as a quoted string.
 */
export const cellWrite = (cell, text) =>
  (cell?.state ?? 'text') === 'text' ? { text } : { expr: text };

/**
 * Which control a field wants, given its cell — the one decision every
 * form makes, kept pure so it is tested without rendering anything:
 *
 * - `computed` — read-only, with a note pointing at the source editor
 * - `symbol` — a picker over the field's symbol set
 * - `idref` — a picker over the ids the caller says this field may name
 * - `bool` — a checkbox
 * - `suggest` — a picker over the values already in use, plus a custom escape
 * - `list` — one comma-separated line of members
 * - `text` — a plain input
 *
 * `ids` and `suggestions` are what the HOST can offer (the model's ids, the
 * kind's `suggestions`); `custom` is set once the user picks "Custom…" and
 * wants to type instead.
 */
export function controlFor(field, cell, { ids, suggestions, custom } = {}) {
  const state = cell?.state ?? 'absent';
  if (isList(field)) return state === 'absent' || state === 'list' ? 'list' : 'computed';
  if (!formEditable(state)) return 'computed';
  if (field?.symbols?.length) return 'symbol';
  if (ids?.length) return 'idref';
  if (bareType(field) === 'bool') return 'bool';
  if (!custom && suggestions?.length && isText(bareType(field))) return 'suggest';
  return 'text';
}

/** Is this field list-valued? A list is edited as one line, not a control
    per member. */
export const isList = (field) => /^list</.test(bareType(field));

/** The block op that writes `text` into `field`, typed from the schema. */
export function valueOp(span, field, text) {
  const ty = bareType(field);
  if (isSlot(field)) {
    const op = { op: 'set_label', span, slot: field.inline_slot };
    if (isText(ty)) return { ...op, text };
    return { ...op, expr: text };
  }
  const op = { op: 'set_field', span, field: field.name };
  if (isText(ty)) return { ...op, text };
  if (ty === 'bool') return { ...op, expr: text === 'true' ? 'true' : 'false' };
  // A cell's symbol text is the bare name, so writing one back re-adds the
  // colon — for an open `symbol` field as much as for a symbol set's.
  if (field.symbols || ty === 'symbol') return { ...op, expr: `:${text}` };
  if (isInt(ty)) return { ...op, expr: String(Math.round(Number(text))) };
  if (isFloat(ty)) {
    const n = Number(text);
    return { ...op, expr: Number.isInteger(n) ? `${n}.0` : String(n) };
  }
  // identifier, and anything unknown: trust the author's raw expression.
  return { ...op, expr: text };
}

/**
 * The ops a draft produces against one block — the ONE save rule, so a
 * change to it lands everywhere at once:
 *
 * - a value equal to what is already there writes nothing (a form that
 *   commits one field must not rewrite the rest of the block)
 * - clearing an OPTIONAL field removes it; clearing a REQUIRED one (or an
 *   inline label, which cannot be absent) is ignored
 * - anything else writes the typed value ({@link valueOp}, or a list
 *   literal for a `list<…>` field)
 *
 * The clear branch runs before the list one, so an emptied optional list is
 * REMOVED like every other emptied optional field rather than being the one
 * field in the editor that clears to `[]`. A required list still clears to
 * `[]`, because an empty list is a value it can legally hold — which is
 * exactly what a required scalar lacks, and why clearing one is ignored.
 *
 * `fields` is the kind's schema metadata, `cells` the instance's current
 * values, `draft` the touched fields by name, `span` the block to write.
 */
export function draftOps(fields, cells, draft, span) {
  const ops = [];
  for (const f of orderFields(fields ?? [])) {
    if (!(f.name in draft)) continue;
    const text = draft[f.name];
    if (text === fieldText(f, cells)) continue;
    if (text === '') {
      if (isSlot(f)) continue;
      if (f.optional !== false) ops.push({ op: 'remove_field', span, field: f.name });
      else if (isList(f)) ops.push({ op: 'set_field', span, field: f.name, expr: '[]' });
      continue;
    }
    if (isList(f)) {
      ops.push({ op: 'set_field', span, field: f.name, expr: listExpr(f, text) });
      continue;
    }
    ops.push(valueOp(span, f, text));
  }
  return ops;
}

/** A plausible id from a display name: lowercase, non-identifier → `_`.
    Every create form derives its id this way until the author types one. */
export const slugify = (name) =>
  String(name ?? '')
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .replace(/^(\d)/, '_$1');

/** The `fields` payload for `/api/unit/create` from a create form's draft:
    the same typing as {@link valueOp}, in the create path's JSON shape.
    Empty entries are left out — the block is written without them. */
export function createFields(fields, draft) {
  const out = {};
  for (const f of fields ?? []) {
    if (isSlot(f)) continue;
    const text = draft?.[f.name];
    if (text == null || text === '') continue;
    out[f.name] = createValue(f, text);
  }
  return out;
}

/** The JSON value for a create-form field (`/api/unit/create`'s `fields`),
    typed the same way {@link valueOp} types an edit: identifiers and symbols
    keep their WCL shape rather than becoming quoted strings, and a type
    neither of them recognises is the author's raw expression on both paths —
    creating and editing a column must not disagree about how it is written. */
export function createValue(field, text) {
  const ty = bareType(field);
  if (isText(ty)) return text;
  if (ty === 'identifier') return { ident: text };
  if (field.symbols || ty === 'symbol') return { sym: text };
  if (ty === 'bool') return text === 'true';
  if (isInt(ty) || isFloat(ty)) {
    const n = Number(text);
    return Number.isNaN(n) ? text : n;
  }
  // A list is the same comma-separated line the edit forms take, typed per
  // element — a quoted string here would be rejected by the field's type.
  if (isList(field)) {
    const elem = { name: field.name, type: listElem(field) };
    return String(text)
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((s) => createValue(elem, s));
  }
  return { expr: text };
}

/** The option value that means "let me type something new". */
export const CUSTOM_OPTION = ' custom';

/**
 * Options for a free-text field the server found a vocabulary for — the
 * values already used by other instances of the kind (`schema.suggestions`).
 * The list always ends with a Custom entry: these fields are open by
 * definition (a `component`'s `kind` is free text), so picking from what
 * exists must never stop you naming something new.
 *
 * `null` when the field has no suggestions and should stay a plain input.
 */
export function suggestOptions(field, schema, currentText) {
  const values = schema?.suggestions?.[field.name] ?? [];
  if (!values.length || !isText(bareType(field))) return null;
  const opts = values.map((v) => ({ value: v, label: v }));
  if (currentText && !values.includes(currentText)) {
    opts.unshift({ value: currentText, label: currentText });
  }
  if (field.optional !== false) opts.unshift({ value: '', label: '(unset)' });
  opts.push({ value: CUSTOM_OPTION, label: '＋ Custom…' });
  return opts;
}

/** The element type of a `list<…>` field ('' when it isn't a list). */
export const listElem = (field) => bareType(field).match(/^list<(.+)>$/)?.[1] ?? '';

/** A WCL string literal (quotes and backslashes escaped). */
export const wclString = (s) => `"${String(s ?? '').replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;

/** `a, b` → the WCL list literal a `list<…>` field of this type wants. */
export function listExpr(field, text) {
  const elem = listElem(field);
  const items = String(text ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  const lit = (s) =>
    elem === 'identifier' || elem === 'bool' || isInt(elem) || isFloat(elem)
      ? s
      : wclString(s);
  return `[${items.map(lit).join(', ')}]`;
}

/** A numeric literal matching the field's declared type. */
const shapeNum = (field, v) =>
  /^[iu]/.test((field?.type ?? 'f64').replace(/\?$/, ''))
    ? String(Math.round(v))
    : Number.isInteger(v)
      ? `${v}.0`
      : String(v);

/**
 * An insertion snippet for a diagram-shape kind from its schema entry:
 * inline slots filled (identifier → a fresh id, string → a placeholder),
 * required fields defaulted, and — under a manual layout — a staggered
 * x/y (or cx/cy) so consecutive adds don't stack at the origin. An
 * explicit `at: {x, y}` (a drop point, in user units) places the shape
 * there instead of the stagger. Shared by the add-shape modal and the
 * screen editor's widget palette.
 */
export function shapeSnippet(entry, { uid, manual, index, at }) {
  const fields = entry.fields ?? [];
  const byName = (n) => fields.find((f) => f.name === n);
  const bare = (f) => (f.type ?? '').replace(/\?$/, '');

  const labels = [];
  let usedId = false;
  for (const f of [...fields].sort((a, b) => (a.inline_slot ?? 0) - (b.inline_slot ?? 0))) {
    if (!isSlot(f)) continue;
    if (bare(f) === 'identifier') {
      labels.push(uid);
      usedId = true;
    } else if (bare(f).startsWith('utf8') || bare(f).startsWith('ascii')) {
      labels.push(wclString('Label'));
    } else if (!f.optional && f.default == null) {
      labels.push('0');
    }
  }

  const body = [];
  if (!usedId && byName('id')) body.push(`id = ${uid}`);
  if (manual || at) {
    const off = 20 + 24 * ((index ?? 0) % 8);
    const pos =
      byName('x') && byName('y') ? ['x', 'y'] : byName('cx') && byName('cy') ? ['cx', 'cy'] : [];
    pos.forEach((name, i) => {
      // Drop points arrive as raw floats — a tenth of a unit is plenty.
      const v = at ? Math.round((i === 0 ? at.x : at.y) * 10) / 10 : off;
      body.push(`${name} = ${shapeNum(byName(name), v)}`);
    });
  }
  // Required, defaultless fields must be present for the commit to validate.
  for (const f of fields) {
    if (isSlot(f) || f.optional || f.default != null) continue;
    if (body.some((line) => line.startsWith(`${f.name} `))) continue;
    const ty = bare(f);
    if (ty.startsWith('utf8') || ty.startsWith('ascii')) body.push(`${f.name} = ${wclString('')}`);
    else if (ty === 'bool') body.push(`${f.name} = false`);
    else if (f.symbols?.length) body.push(`${f.name} = :${f.symbols[0]}`);
    else if (ty === 'identifier') body.push(`${f.name} = ${uid}_ref`);
    else body.push(`${f.name} = ${shapeNum(f, 0)}`);
  }

  const head = [entry.kind, ...labels].join(' ');
  return body.length ? `${head} {\n${body.map((l) => `  ${l}`).join('\n')}\n}` : `${head} {}`;
}

/** A fresh `<kind>_<n>` id dodging every id already used in `source` —
    `id = …` fields and inline block labels alike (diagram-node ids share
    one space per document). */
export function freshShapeId(kind, source) {
  const used = new Set(
    [...String(source ?? '').matchAll(/\bid\s*=\s*([A-Za-z_]\w*)/g)]
      .map((m) => m[1])
      .concat(
        [...String(source ?? '').matchAll(/^\s*[a-z_]\w*\s+([a-z_]\w*)\b/gm)].map((m) => m[1]),
      ),
  );
  let n = 1;
  while (used.has(`${kind}_${n}`)) n += 1;
  return `${kind}_${n}`;
}

/**
 * A new block of `entry`'s kind as WCL source: the inline id slot filled,
 * every required field given a value of the right shape. Used to seed a
 * child block (`insert_child`) from nothing but its schema.
 */
export function blockSnippet(entry, { id, values = {} } = {}) {
  const fields = entry?.fields ?? [];
  const labels = [];
  const body = [];
  for (const f of orderFields(fields)) {
    const ty = bareType(f);
    const given = values[f.name];
    if (isSlot(f)) {
      labels.push(ty === 'identifier' ? (given ?? id) : wclString(given ?? id));
      continue;
    }
    const needed = f.optional === false && f.default == null;
    if (given == null && !needed) continue;
    if (ty === 'identifier') body.push(`${f.name} = ${given ?? id}`);
    else if (f.symbols?.length) body.push(`${f.name} = :${given ?? f.symbols[0]}`);
    else if (ty === 'bool') body.push(`${f.name} = ${given === 'true' || given === true}`);
    else if (isInt(ty) || isFloat(ty)) body.push(`${f.name} = ${Number(given ?? 0) || 0}`);
    else if (ty.startsWith('list<')) body.push(`${f.name} = []`);
    else body.push(`${f.name} = ${wclString(given ?? '')}`);
  }
  const head = [entry.kind, ...labels].join(' ');
  return body.length ? `${head} {\n${body.map((l) => `  ${l}`).join('\n')}\n}` : `${head} {}`;
}
