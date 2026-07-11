/* Forge code editor — optional copy-in asset wrapping CodeMirror 6.
   Needs console.css's "Code editor" + "Popovers & menus" sections, and these
   npm packages (install when you copy this file — see reference/solidjs.md):

     @codemirror/state @codemirror/view @codemirror/language
     @codemirror/commands @codemirror/lint @codemirror/search
     @codemirror/merge @codemirror/lang-javascript @codemirror/lang-python
     @codemirror/lang-json @codemirror/lang-css @codemirror/lang-html
     @codemirror/legacy-modes @lezer/highlight

   Exports: CodeEditor, DiffEditor, LANGUAGES, forgeTheme.
   Annotation positions: 1-based line, 0-based col (LSP convention).
   Imports nothing from ui.jsx. */

import { Show, For, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { EditorState, Compartment } from '@codemirror/state';
import {
  EditorView, keymap, lineNumbers as lineNumbersExt, placeholder as placeholderExt,
  highlightActiveLine, highlightActiveLineGutter, drawSelection,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { syntaxHighlighting, HighlightStyle, StreamLanguage, bracketMatching } from '@codemirror/language';
import { setDiagnostics, lintGutter } from '@codemirror/lint';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { MergeView, unifiedMergeView } from '@codemirror/merge';
import { javascript } from '@codemirror/lang-javascript';
import { python } from '@codemirror/lang-python';
import { json } from '@codemirror/lang-json';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { tags as t } from '@lezer/highlight';

/* ---------------- Languages ------------------------------------------------- */
export const LANGUAGES = {
  js: () => javascript(),
  jsx: () => javascript({ jsx: true }),
  ts: () => javascript({ typescript: true }),
  tsx: () => javascript({ typescript: true, jsx: true }),
  python: () => python(),
  json: () => json(),
  css: () => css(),
  html: () => html(),
  shell: () => StreamLanguage.define(shell),
};
const resolveLanguage = (lang) => (lang && LANGUAGES[lang] ? LANGUAGES[lang]() : []);

/* ---------------- Forge theme (every value a var(--token)) ------------------ */
const forgeEditorTheme = EditorView.theme({
  '&': { background: 'var(--bg-0)', color: 'var(--fg-0)', fontSize: '12px', height: '100%' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.6' },
  '.cm-content': { caretColor: 'var(--accent)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-gutters': {
    background: 'var(--bg-0)', color: 'var(--fg-3)',
    borderRight: '1px solid var(--border-subtle)',
  },
  '.cm-activeLine': { background: 'var(--bg-1)' },
  '.cm-activeLineGutter': { background: 'var(--bg-1)', color: 'var(--fg-1)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection':
    { background: 'var(--accent-bg) !important' },
  '.cm-selectionMatch': { background: 'var(--accent-bg)' },
  '.cm-matchingBracket': { background: 'var(--bg-3)', outline: '1px solid var(--border-strong)' },
  '.cm-panels': { background: 'var(--bg-1)', color: 'var(--fg-0)', border: '1px solid var(--border)' },
  '.cm-searchMatch': { background: 'var(--warning-bg)' },
  '.cm-searchMatch-selected': { background: 'var(--accent-bg)' },
  '.cm-tooltip': {
    background: 'var(--bg-4)', color: 'var(--fg-0)',
    border: '1px solid var(--border-strong)', borderRadius: 'var(--r-md)', fontSize: '12px',
  },
  '.cm-lintRange-error': { textDecoration: 'underline wavy var(--danger)' },
  '.cm-lintRange-warning': { textDecoration: 'underline wavy var(--warning)' },
  '.cm-lintRange-info': { textDecoration: 'underline wavy var(--info)' },
  '.cm-lintRange-hint': { textDecoration: 'underline dotted var(--fg-3)' },
  '.cm-lint-marker-error': { content: 'none', background: 'var(--danger)', borderRadius: '999px', width: '8px', height: '8px' },
  '.cm-lint-marker-warning': { content: 'none', background: 'var(--warning)', borderRadius: '999px', width: '8px', height: '8px' },
  '.cm-lint-marker-info': { content: 'none', background: 'var(--info)', borderRadius: '999px', width: '8px', height: '8px' },
  /* merge view */
  '.cm-mergeView': { background: 'var(--bg-0)' },
  '.cm-merge-a .cm-changedLine, .cm-deletedChunk': { background: 'var(--danger-bg)' },
  '.cm-merge-b .cm-changedLine, .cm-insertedLine': { background: 'var(--success-bg)' },
  '.cm-changedText': { background: 'color-mix(in oklab, var(--success) 30%, transparent)' },
  '.cm-deletedText': { background: 'color-mix(in oklab, var(--danger) 30%, transparent)' },
});

const forgeHighlight = HighlightStyle.define([
  { tag: [t.keyword, t.moduleKeyword, t.operatorKeyword], color: 'var(--accent-fg)' },
  { tag: [t.string, t.special(t.string)], color: 'var(--success-fg)' },
  { tag: [t.number, t.bool, t.atom, t.null], color: 'var(--info-fg)' },
  { tag: [t.comment, t.docComment], color: 'var(--fg-3)' },
  { tag: [t.typeName, t.className, t.namespace], color: 'var(--warning-fg)' },
  { tag: [t.propertyName, t.attributeName], color: 'var(--info-fg)' },
  { tag: [t.regexp, t.escape], color: 'var(--warning-fg)' },
  { tag: t.tagName, color: 'var(--accent-fg)' },
  { tag: t.variableName, color: 'var(--fg-0)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--fg-0)', fontWeight: '500' },
  { tag: [t.meta, t.punctuation, t.operator], color: 'var(--fg-2)' },
  { tag: t.invalid, color: 'var(--danger-fg)' },
  { tag: t.heading, color: 'var(--fg-0)', fontWeight: '700' },
  { tag: [t.link, t.url], color: 'var(--accent-fg)', textDecoration: 'underline' },
]);

export const forgeTheme = [forgeEditorTheme, syntaxHighlighting(forgeHighlight)];

/* ---------------- annotation mapping ---------------------------------------- */
const toDiagnostics = (state, anns) => {
  const doc = state.doc;
  const pos = (p) => {
    const ln = doc.line(Math.min(Math.max(1, p.line), doc.lines));
    return Math.min(ln.from + Math.max(0, p.col ?? 0), ln.to);
  };
  return (anns ?? []).map((a) => {
    const from = pos(a.from);
    const to = a.to ? Math.max(pos(a.to), from) : Math.min(from + 1, doc.length);
    return { from, to, severity: a.severity ?? 'info', message: a.message, source: a.source };
  });
};

/* Forge context menu rendered at the pointer (uses round-4 .fmenu classes). */
function CodeMenu(props) {
  createEffect(() => {
    if (!props.pos()) return;
    const close = () => props.setPos(null);
    const onDown = (e) => { if (!e.target.closest('.fcode-menu')) close(); };
    const onKey = (e) => { if (e.key === 'Escape') close(); };
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onKey);
    onCleanup(() => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onKey);
    });
  });
  return (
    <Show when={props.pos()}>
      <Portal>
        <div class="fpop fmenu-pop fcode-menu" role="menu"
             style={{
               left: `${Math.min(props.pos().x, window.innerWidth - 200)}px`,
               top: `${Math.min(props.pos().y, window.innerHeight - 40 * (props.items?.length ?? 1))}px`,
             }}>
          <For each={props.items}>
            {(item) => (
              <Show when={!item.separator} fallback={<div class="fmenu-sep" role="separator" />}>
                <button type="button" class="fmenu-item" role="menuitem" disabled={item.disabled}
                        classList={{ 'is-danger': !!item.danger, 'is-disabled': !!item.disabled }}
                        onClick={() => { props.setPos(null); item.onSelect?.(props.view()); }}>
                  <span class="fmenu-label">{item.label}</span>
                  <Show when={item.kbd}>
                    <span class="fmenu-kbd">{item.kbd}</span>
                  </Show>
                </button>
              </Show>
            )}
          </For>
        </div>
      </Portal>
    </Show>
  );
}

/* ---------------- CodeEditor ------------------------------------------------- */
export function CodeEditor(props) {
  let host, view;
  const lang = new Compartment(), editable = new Compartment(), wrapping = new Compartment();
  const [menu, setMenu] = createSignal(null);
  const isRO = () => props.readOnly || !props.onChange;
  const roExt = () => [EditorState.readOnly.of(isRO()), EditorView.editable.of(!isRO())];

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: props.value ?? '',
        extensions: [
          ...forgeTheme,
          props.lineNumbers !== false ? [lineNumbersExt(), highlightActiveLineGutter()] : [],
          highlightActiveLine(), drawSelection(), bracketMatching(),
          lintGutter(), history(), highlightSelectionMatches(),
          keymap.of([...defaultKeymap, ...searchKeymap, ...historyKeymap, indentWithTab]),
          props.placeholder ? placeholderExt(props.placeholder) : [],
          lang.of(resolveLanguage(props.language)),
          editable.of(roExt()),
          wrapping.of(props.wrap ? EditorView.lineWrapping : []),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) props.onChange?.(u.state.doc.toString());
          }),
          EditorView.domEventHandlers({
            contextmenu: (e) => {
              if (!props.contextMenuItems?.length) return false;
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY });
              return true;
            },
          }),
        ],
      }),
    });
  });
  onCleanup(() => view?.destroy());

  /* External value → view; compare-before-dispatch keeps the loop safe. */
  createEffect(() => {
    const next = props.value ?? '';
    if (view && next !== view.state.doc.toString())
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
  });
  /* Annotations → lint diagnostics (re-mapped when the doc swaps). */
  createEffect(() => {
    const anns = props.annotations ?? [];
    props.value;
    if (view) view.dispatch(setDiagnostics(view.state, toDiagnostics(view.state, anns)));
  });
  createEffect(() => view?.dispatch({ effects: editable.reconfigure(roExt()) }));
  createEffect(() => view?.dispatch({ effects: lang.reconfigure(resolveLanguage(props.language)) }));
  createEffect(() => view?.dispatch({ effects: wrapping.reconfigure(props.wrap ? EditorView.lineWrapping : []) }));

  return (
    <div class={`fcode ${props.class ?? ''}`}
         style={{ height: props.height ?? '240px', ...props.style }} ref={host}>
      <CodeMenu pos={menu} setPos={setMenu} items={props.contextMenuItems} view={() => view} />
    </div>
  );
}

/* ---------------- DiffEditor ------------------------------------------------- */
export function DiffEditor(props) {
  let host, mv, uview;

  const shared = () => [
    ...forgeTheme,
    props.lineNumbers !== false ? lineNumbersExt() : [],
    props.wrap ? EditorView.lineWrapping : [],
    lang0.of(resolveLanguage(props.language)),
  ];
  const lang0 = new Compartment();

  const build = () => {
    mv?.destroy(); uview?.destroy(); mv = uview = null;
    host.textContent = '';
    if (props.unified) {
      uview = new EditorView({
        parent: host,
        state: EditorState.create({
          doc: props.modified ?? '',
          extensions: [
            ...shared(),
            unifiedMergeView({ original: props.original ?? '' }),
            EditorState.readOnly.of(!props.onChange),
            EditorView.editable.of(!!props.onChange),
            EditorView.updateListener.of((u) => {
              if (u.docChanged) props.onChange?.(u.state.doc.toString());
            }),
          ],
        }),
      });
    } else {
      mv = new MergeView({
        parent: host,
        a: {
          doc: props.original ?? '',
          extensions: [...shared(), EditorState.readOnly.of(true), EditorView.editable.of(false)],
        },
        b: {
          doc: props.modified ?? '',
          extensions: [
            ...shared(),
            EditorState.readOnly.of(!props.onChange),
            EditorView.editable.of(!!props.onChange),
            EditorView.updateListener.of((u) => {
              if (u.docChanged) props.onChange?.(u.state.doc.toString());
            }),
          ],
        },
      });
    }
  };

  onMount(() => {
    build();
    /* rebuild when the layout mode flips */
    createEffect((prev) => {
      const mode = !!props.unified;
      if (prev !== undefined && mode !== prev) build();
      return mode;
    });
    /* external doc updates, compare-before-dispatch */
    createEffect(() => {
      const a = props.original ?? '';
      const av = mv?.a;
      if (av && a !== av.state.doc.toString())
        av.dispatch({ changes: { from: 0, to: av.state.doc.length, insert: a } });
    });
    createEffect(() => {
      const b = props.modified ?? '';
      const bv = mv?.b ?? uview;
      if (bv && b !== bv.state.doc.toString())
        bv.dispatch({ changes: { from: 0, to: bv.state.doc.length, insert: b } });
    });
    /* annotations on the modified side */
    createEffect(() => {
      const anns = props.annotations ?? [];
      const bv = mv?.b ?? uview;
      if (bv) bv.dispatch(setDiagnostics(bv.state, toDiagnostics(bv.state, anns)));
    });
  });
  onCleanup(() => { mv?.destroy(); uview?.destroy(); });

  return (
    <div class={`fcode fcode-diff ${props.class ?? ''}`}
         style={{ height: props.height ?? '280px', ...props.style }} ref={host} />
  );
}
