/* Operation building for the five diagram-shape gestures — move, resize,
   connect, relocate and convert-to-manual — as one pure entry point.

   Given a shape's `/api/block/source` payload, its palette kind definition
   (`diagram_kinds[]`: `{ kind, fields: [{ name, type }] }`) and the gesture,
   `shapeOps` returns either the ops to commit or a refusal carrying a
   machine-readable `reason` and the message to show. The caller keeps the
   source fetch, the commit, the toasts and every DOM read (translate,
   client→user coordinates, the rendered bbox) — nothing here touches a
   document, so the schema guards and the geometry maths are assertable
   without a browser.

   The geometry primitives themselves live in ./diagram (resizeDelta,
   readTranslate, clientToUser); this is the other half — the schema guards
   and the op synthesis that used to sit inline in EditSurface. */

import { isManualLayout } from './diagram';
import { relocateOps } from './widgetdnd';

/** A resize never takes a shape below this, in user units. */
const MIN_SIZE = 8;

const refuse = (reason, message) => ({ ok: false, reason, message });

/**
 * Build the ops for one shape gesture.
 *
 * Every input carries `gesture`; the rest is per-gesture (see the builders
 * below). Returns `{ ok: true, ops, ... }` or
 * `{ ok: false, reason, message }`.
 */
export function shapeOps(input) {
  switch (input?.gesture) {
    case 'move':
      return moveOps(input);
    case 'resize':
      return resizeOps(input);
    case 'connect':
      return connectOps(input);
    case 'relocate':
      return relocateGestureOps(input);
    case 'convert':
      return convertOps(input);
    default:
      return refuse('unknown-gesture', `unknown shape gesture: ${input?.gesture}`);
  }
}

// --- schema / value helpers -----------------------------------------------

/** The declared type of a kind's field, or null when it has no such field. */
function fieldType(kindDef, name) {
  return kindDef?.fields?.find((f) => f.name === name)?.type ?? null;
}

/** Does this shape kind declare `name`? (An unknown kind declares nothing.) */
function hasField(kindDef, name) {
  return fieldType(kindDef, name) != null;
}

/** A numeric field expr matching the schema's declared type: integer kinds
    get integers, everything else a decimal so f64 fields stay float-typed. */
function numExpr(kindDef, name, v) {
  const ty = fieldType(kindDef, name) ?? 'f64';
  if (/^[iu]/.test(ty)) return String(Math.round(v));
  const r = Math.round(v * 10) / 10;
  return Number.isInteger(r) ? `${r}.0` : String(r);
}

/** Current numeric value of a shape field from a block-source payload:
    absent → 0, number literal → its value, anything else → NaN (computed,
    so not form-writable). */
function numField(source, name) {
  const slot = source?.fields?.[name];
  if (!slot) return 0;
  return slot.state === 'number' ? Number(slot.text) : NaN;
}

/** Is the field written at all (absent fields fall back to the render)? */
const isSet = (source, name) => !!source?.fields?.[name];

// --- the gestures ---------------------------------------------------------

/** Drag released on a free/none-layout diagram: shift x/y by the user-unit
    delta. `{ kind, kindDef, span, source, delta: { dx, dy } }` */
function moveOps({ kind, kindDef, span, source, delta }) {
  if (!hasField(kindDef, 'x') || !hasField(kindDef, 'y')) {
    return refuse('no-position-field', `a ${kind} has no x/y — edit its source instead`);
  }
  const x0 = numField(source, 'x');
  const y0 = numField(source, 'y');
  if (Number.isNaN(x0) || Number.isNaN(y0)) {
    return refuse('computed-position', 'x/y are computed — edit the source instead');
  }
  return {
    ok: true,
    ops: [
      { op: 'set_field', span, field: 'x', expr: numExpr(kindDef, 'x', x0 + delta.dx) },
      { op: 'set_field', span, field: 'y', expr: numExpr(kindDef, 'y', y0 + delta.dy) },
    ],
  };
}

/** Corner handle released: width/height (never below MIN_SIZE) plus the x/y
    a top/left grab implies. `box` is the rendered bbox, used only for
    dimensions the source doesn't set — so the first grab of a default-sized
    shape doesn't collapse it, while a COMPUTED one still refuses.
    `{ kind, kindDef, span, source, delta: { dx, dy, dw, dh }, box }` */
function resizeOps({ kind, kindDef, span, source, delta, box }) {
  if (!hasField(kindDef, 'width') || !hasField(kindDef, 'height')) {
    return refuse('no-size-field', `a ${kind} has no width/height — edit its source instead`);
  }
  const vals = {
    x: numField(source, 'x'),
    y: numField(source, 'y'),
    width: isSet(source, 'width') ? numField(source, 'width') : (box?.width ?? 0),
    height: isSet(source, 'height') ? numField(source, 'height') : (box?.height ?? 0),
  };
  const ops = [];
  for (const [name, v] of [
    ['width', Math.max(vals.width + delta.dw, MIN_SIZE)],
    ['height', Math.max(vals.height + delta.dh, MIN_SIZE)],
  ]) {
    if (Number.isNaN(vals[name])) {
      return refuse('computed-size', `${name} is computed — edit the source instead`);
    }
    ops.push({ op: 'set_field', span, field: name, expr: numExpr(kindDef, name, v) });
  }
  // A top/left grab also moves the shape; a computed x/y just keeps its
  // place (the size edit is still worth making).
  for (const [name, d] of [
    ['x', delta.dx],
    ['y', delta.dy],
  ]) {
    if (!d || !hasField(kindDef, name) || Number.isNaN(vals[name])) continue;
    ops.push({ op: 'set_field', span, field: name, expr: numExpr(kindDef, name, vals[name] + d) });
  }
  return { ok: true, ops };
}

/** Port dropped on another shape: an `a -> b` connection statement. The op
    addresses the DIAGRAM (connections are its items), not either shape.
    `{ from, to, owner: { span, shared } | null }` */
function connectOps({ from, to, owner }) {
  if (!from || !to) {
    return refuse('missing-id', 'Both shapes need an id before they can be connected');
  }
  if (from === to) {
    return refuse('self-connection', 'A shape cannot be connected to itself');
  }
  if (!owner) return refuse('no-diagram', 'No diagram found for these shapes');
  if (owner.shared) return refuse('generated', GENERATED);
  return { ok: true, ops: [{ op: 'connect_add', span: owner.span, from, to }] };
}

const GENERATED = 'This diagram is generated — edit its source data instead';

/** A move released somewhere structural: re-home the shape's canonical
    source slice — after a leaf sibling, into a container widget, or out onto
    the diagram. `at` (user units) only survives when the target actually
    honours per-shape coordinates, so a drop on a solver-laid-out diagram
    doesn't write a position the layout would ignore — the move still
    happens; `positional` reports whether the drop point survived it.
    `{ slice, mode: 'after'|'inside'|'diagram', source: { file, span, shared },
       target: { kind, file, span, shared, layout, acceptsChildren }, at, slot }` */
function relocateGestureOps({ slice, mode, source, target, at = null, slot = null }) {
  if (source?.shared || target?.shared) return refuse('generated', GENERATED);
  if (source?.file !== target?.file) {
    return refuse('cross-file', 'Cannot move a widget across files — edit the source instead');
  }
  // A drop resolved without consulting the schema (or one whose target kind
  // stopped taking children): nesting there would not render.
  if (mode === 'inside' && target.acceptsChildren === false) {
    return refuse(
      'target-rejects-children',
      `a ${target.kind ?? 'shape'} cannot contain other widgets`,
    );
  }
  const positional = mode === 'diagram' && isManualLayout(target.layout);
  return {
    ok: true,
    positional,
    ops: relocateOps({
      slice,
      mode,
      targetSpan: target.span,
      sourceSpan: source.span,
      at: positional ? at : null,
      slot,
    }),
  };
}

/** Convert a solver-laid-out diagram to manual placement: switch it to
    `:free` and materialize every listed child's CURRENT position into
    explicit x/y, so the diagram doesn't rearrange itself. Children whose
    kind has no x/y keep their defaults and are counted in `skipped`.
    `{ span, children: [{ kindDef, span, at: { x, y } }] }` */
function convertOps({ span, children }) {
  const ops = [{ op: 'set_field', span, field: 'layout', expr: ':free' }];
  let skipped = 0;
  for (const child of children ?? []) {
    if (!hasField(child.kindDef, 'x') || !hasField(child.kindDef, 'y')) {
      skipped += 1;
      continue;
    }
    ops.push(
      {
        op: 'set_field',
        span: child.span,
        field: 'x',
        expr: numExpr(child.kindDef, 'x', child.at.x),
      },
      {
        op: 'set_field',
        span: child.span,
        field: 'y',
        expr: numExpr(child.kindDef, 'y', child.at.y),
      },
    );
  }
  return { ok: true, ops, skipped };
}
