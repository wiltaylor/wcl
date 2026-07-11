/* Forge UI primitives — SolidJS port of ui_kits/console/ui.jsx.
   Requires console.css + colors_and_type.css to be imported by the app.
   Icons come from lucide-solid: pass the imported icon component via the
   `icon` prop, e.g. icon={Terminal}. All icons render at 1.5px stroke. */

import { Show, For, Index, createSignal, createEffect, createUniqueId, onCleanup, mergeProps, splitProps } from 'solid-js';
import { Dynamic, Portal } from 'solid-js/web';

/* ---------------- Icon ----------------------------------------------------- */
/* Wraps a lucide-solid icon component with Forge defaults (16px, 1.5 stroke). */
export function Icon(props) {
  const merged = mergeProps({ size: 16 }, props);
  const [local, rest] = splitProps(merged, ['of', 'size']);
  return <Dynamic component={local.of} size={local.size} strokeWidth={1.5} aria-hidden="true" {...rest} />;
}

/* ---------------- Button --------------------------------------------------- */
export function Button(props) {
  const merged = mergeProps({ variant: 'secondary', size: 'md' }, props);
  const [local, rest] = splitProps(merged, ['variant', 'size', 'icon', 'children', 'class']);
  return (
    <button
      class={`fbtn fbtn-${local.variant} fbtn-${local.size} ${local.class ?? ''}`}
      {...rest}
    >
      <Show when={local.icon}>
        <Icon of={local.icon} size={local.size === 'sm' ? 12 : 14} />
      </Show>
      {local.children}
    </button>
  );
}

/* ---------------- Input ---------------------------------------------------- */
export function Input(props) {
  const [local, rest] = splitProps(props, ['icon', 'error', 'label', 'help']);
  return (
    <label class="ffield">
      <Show when={local.label}>
        <span class="ffield-label">{local.label}</span>
      </Show>
      <span class="ffield-input" classList={{ 'is-error': !!local.error }}>
        <Show when={local.icon}>
          <Icon of={local.icon} size={14} />
        </Show>
        <input {...rest} />
      </span>
      <Show when={local.help}>
        <span class="ffield-help" classList={{ 'is-error': !!local.error }}>{local.help}</span>
      </Show>
    </label>
  );
}

/* ---------------- Badge ---------------------------------------------------- */
export function Badge(props) {
  const merged = mergeProps({ tone: 'neutral' }, props);
  return (
    <span class={`fbadge fbadge-${merged.tone}`}>
      <Show when={merged.dot}>
        <span class="fbadge-dot" />
      </Show>
      {merged.children}
    </span>
  );
}

/* ---------------- Card ----------------------------------------------------- */
export function Card(props) {
  const merged = mergeProps({ padded: true }, props);
  return (
    <section class={`fcard ${merged.class ?? ''}`}>
      <Show when={merged.title}>
        <header class="fcard-head">
          <h3 class="fcard-title">{merged.title}</h3>
          {merged.action}
        </header>
      </Show>
      <div class={merged.padded ? 'fcard-body' : ''}>{merged.children}</div>
    </section>
  );
}

/* ---------------- Stat ----------------------------------------------------- */
export function Stat(props) {
  return (
    <div class="fstat">
      <div class="eyebrow">{props.label}</div>
      <div class="fstat-value">{props.value}</div>
      <Show when={props.delta}>
        <div class={`fstat-delta tone-${props.tone ?? 'neutral'}`}>{props.delta}</div>
      </Show>
    </div>
  );
}

/* ---------------- Kbd ------------------------------------------------------ */
export function Kbd(props) {
  return <kbd class="fkbd">{props.children}</kbd>;
}

/* ---------------- Toast ---------------------------------------------------- */
export function Toast(props) {
  const merged = mergeProps({ tone: 'info' }, props);
  return (
    <div class={`ftoast ftoast-${merged.tone}`}>
      <Show when={merged.icon}>
        <Icon of={merged.icon} size={14} />
      </Show>
      <span>{merged.children}</span>
    </div>
  );
}

/* ---------------- Status dot ---------------------------------------------- */
export function StatusDot(props) {
  return <span class={`fdot fdot-${props.tone}`} />;
}

/* ===========================================================================
   Shell, page, data and settings components — thin wrappers over the
   console.css classes. Raw-class markup remains fully supported; these are
   additive sugar. Private SVGs below keep ui.jsx usable without lucide-solid.
   =========================================================================== */

const MenuSvg = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
    <line x1="4" y1="6" x2="20" y2="6" /><line x1="4" y1="12" x2="20" y2="12" /><line x1="4" y1="18" x2="20" y2="18" />
  </svg>
);
const XSvg = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
    <line x1="6" y1="6" x2="18" y2="18" /><line x1="18" y1="6" x2="6" y2="18" />
  </svg>
);

/* ---------------- App shell ------------------------------------------------ */
/* Owns the mobile drawer state: renders the hamburger toggle, the scrim, and
   closes the drawer when the sidebar is tapped. Inert above 1024px. */
export function AppShell(props) {
  const [navOpen, setNavOpen] = createSignal(false);
  const close = () => setNavOpen(false);
  return (
    <div class="app-shell" classList={{ 'is-sidebar-open': navOpen() }}>
      <header class="ftopbar">
        <button class="ftopbar-icon-btn fsidebar-toggle" aria-label="Toggle navigation"
                aria-expanded={navOpen()} onClick={() => setNavOpen((o) => !o)}>
          <MenuSvg />
        </button>
        {props.topbar}
      </header>
      <nav class="fsidebar" onClick={close}>{props.sidebar}</nav>
      <div class="fscrim" onClick={close} />
      <main class="app-main">{props.children}</main>
    </div>
  );
}

export function NavSection(props) {
  return <div class="fsidebar-section">{props.children}</div>;
}

export function NavLink(props) {
  const [local, rest] = splitProps(props, ['icon', 'active', 'count', 'children']);
  return (
    <a classList={{ 'is-active': !!local.active }} {...rest}>
      <Show when={local.icon}>
        <Icon of={local.icon} size={14} />
      </Show>
      {local.children}
      <Show when={local.count != null}>
        <span class="count">{local.count}</span>
      </Show>
    </a>
  );
}

export function Crumbs(props) {
  return (
    <div class="ftopbar-crumbs">
      <Index each={props.items}>
        {(item, i) => (
          <>
            <Show when={i > 0}>
              <span class="sep">/</span>
            </Show>
            <span>{item()}</span>
          </>
        )}
      </Index>
    </div>
  );
}

export function IconButton(props) {
  const [local, rest] = splitProps(props, ['icon', 'label']);
  return (
    <button class="ftopbar-icon-btn" aria-label={local.label} title={local.label} {...rest}>
      <Icon of={local.icon} />
    </button>
  );
}

/* ---------------- Page ----------------------------------------------------- */
export function PageHead(props) {
  return (
    <div class="page-head">
      <div>
        <h1 style={{ 'font-size': '22px' }}>{props.title}</h1>
        <Show when={props.sub}>
          <div class="sub">{props.sub}</div>
        </Show>
      </div>
      <Show when={props.actions}>
        <div class="page-actions">{props.actions}</div>
      </Show>
    </div>
  );
}

export function Grid(props) {
  const [local, rest] = splitProps(props, ['children']);
  return <div class="fgrid" {...rest}>{local.children}</div>;
}

export function Empty(props) {
  return (
    <div class="empty">
      <h3>{props.title}</h3>
      <Show when={props.children}>
        <p>{props.children}</p>
      </Show>
      {props.action}
    </div>
  );
}

export function Eyebrow(props) {
  return <div class="eyebrow">{props.children}</div>;
}

/* ---------------- Table ---------------------------------------------------- */
/* Markup-only: pass thead/tbody as children (see the dashboard-card example).
   The wrap div gives wide tables horizontal scroll at <=768px. */
export function Table(props) {
  const [local, rest] = splitProps(props, ['children']);
  return (
    <div class="ftable-wrap">
      <table class="ftable" {...rest}>{local.children}</table>
    </div>
  );
}

/* ---------------- Logs ----------------------------------------------------- */
export function Logs(props) {
  const [local, rest] = splitProps(props, ['children']);
  return <div class="flogs" {...rest}>{local.children}</div>;
}

export function LogLine(props) {
  const merged = mergeProps({ level: 'info' }, props);
  return (
    <div class="flog-line">
      <span class="flog-time">{merged.time}</span>
      <span class={`flog-level ${merged.level}`}>{merged.level}</span>
      <span class="flog-msg">{merged.children}</span>
    </div>
  );
}

/* ---------------- Settings ------------------------------------------------- */
export function SettingsLayout(props) {
  return (
    <div class="settings-layout">
      <nav class="settings-nav">{props.nav}</nav>
      <div>{props.children}</div>
    </div>
  );
}

export function SettingsSection(props) {
  return (
    <section class="settings-section">
      <h2>{props.title}</h2>
      <Show when={props.sub}>
        <p class="sub">{props.sub}</p>
      </Show>
      {props.children}
    </section>
  );
}

export function SettingsRow(props) {
  return <div class="settings-row">{props.children}</div>;
}

/* ---------------- Modal ---------------------------------------------------- */
/* Controlled: <Modal open={sig()} onClose={...} title footer>body</Modal>.
   Closes on Escape, backdrop click, and the head X. */
export function Modal(props) {
  createEffect(() => {
    if (!props.open) return;
    const onKey = (e) => { if (e.key === 'Escape') props.onClose?.(); };
    document.addEventListener('keydown', onKey);
    onCleanup(() => document.removeEventListener('keydown', onKey));
  });
  return (
    <Show when={props.open}>
      <Portal>
        <div class="fmodal" onClick={(e) => { if (e.target === e.currentTarget) props.onClose?.(); }}>
          <div class="fmodal-panel" role="dialog" aria-modal="true" aria-label={props.title}>
            <header class="fmodal-head">
              <h3>{props.title}</h3>
              <button class="ftopbar-icon-btn" aria-label="Close" onClick={() => props.onClose?.()}>
                <XSvg />
              </button>
            </header>
            <div class="fmodal-body">{props.children}</div>
            <Show when={props.footer}>
              <footer class="fmodal-foot">{props.footer}</footer>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}

/* ===========================================================================
   Form controls — Checkbox / Toggle / Radio hide a native input (a11y + form
   semantics) and draw the control with console.css classes.
   =========================================================================== */

const CheckMark = () => (
  <svg class="fcheck-mark" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M4 12l5 5L20 6" />
  </svg>
);
const CheckDash = () => (
  <svg class="fcheck-dash" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="3" stroke-linecap="round" aria-hidden="true" style={{ position: 'absolute' }}>
    <line x1="5" y1="12" x2="19" y2="12" />
  </svg>
);
const ChevronDown = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M6 9l6 6 6-6" />
  </svg>
);

/* ---------------- Checkbox ------------------------------------------------- */
export function Checkbox(props) {
  const [local, rest] = splitProps(props, ['checked', 'onChange', 'indeterminate', 'children']);
  let input;
  createEffect(() => { if (input) input.indeterminate = !!local.indeterminate; });
  return (
    <label class="fcheck">
      <input ref={input} type="checkbox" checked={local.checked}
             onInput={(e) => local.onChange?.(e.currentTarget.checked)} {...rest} />
      <span class="fcheck-box"><CheckMark /><CheckDash /></span>
      <Show when={local.children}>
        <span class="fcheck-label">{local.children}</span>
      </Show>
    </label>
  );
}

/* ---------------- Toggle (switch) ------------------------------------------ */
export function Toggle(props) {
  const [local, rest] = splitProps(props, ['checked', 'onChange', 'children']);
  return (
    <label class="ftoggle">
      <input type="checkbox" role="switch" checked={local.checked}
             onInput={(e) => local.onChange?.(e.currentTarget.checked)} {...rest} />
      <span class="ftoggle-track"><span class="ftoggle-knob" /></span>
      <Show when={local.children}>
        <span class="ftoggle-label">{local.children}</span>
      </Show>
    </label>
  );
}

/* ---------------- Radio ---------------------------------------------------- */
export function Radio(props) {
  const [local, rest] = splitProps(props, ['value', 'checked', 'onChange', 'children']);
  return (
    <label class="fradio">
      <input type="radio" value={local.value} checked={local.checked}
             onInput={() => local.onChange?.(local.value)} {...rest} />
      <span class="fradio-dot" />
      <Show when={local.children}>
        <span class="fradio-label">{local.children}</span>
      </Show>
    </label>
  );
}

export function RadioGroup(props) {
  const merged = mergeProps({ name: createUniqueId() }, props);
  return (
    <div class="ffield">
      <Show when={merged.label}>
        <span class="ffield-label">{merged.label}</span>
      </Show>
      <div class="fradio-group" classList={{ 'is-row': !!merged.row }} role="radiogroup">
        <For each={merged.options}>
          {(opt) => (
            <Radio name={merged.name} value={opt.value} checked={merged.value === opt.value}
                   disabled={opt.disabled} onChange={(v) => merged.onChange?.(v)}>
              {opt.label}
            </Radio>
          )}
        </For>
      </div>
    </div>
  );
}

/* ---------------- Select (custom popover dropdown) -------------------------- */
export function Select(props) {
  const [local, rest] = splitProps(props,
    ['options', 'value', 'onChange', 'placeholder', 'label', 'help', 'error', 'children']);
  const [open, setOpen] = createSignal(false);
  const [activeIdx, setActiveIdx] = createSignal(-1);
  let root;

  const selected = () => local.options?.find((o) => o.value === local.value);
  const enabledIdx = (from, dir) => {
    const opts = local.options ?? [];
    for (let i = from; i >= 0 && i < opts.length; i += dir) if (!opts[i].disabled) return i;
    return -1;
  };
  const openAt = () => {
    const cur = local.options?.findIndex((o) => o.value === local.value) ?? -1;
    setActiveIdx(cur >= 0 ? cur : enabledIdx(0, 1));
    setOpen(true);
  };
  const commit = (idx) => {
    const opt = local.options?.[idx];
    if (opt && !opt.disabled) { local.onChange?.(opt.value); setOpen(false); }
  };
  const onKeyDown = (e) => {
    if (!open()) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) { e.preventDefault(); openAt(); }
      return;
    }
    if (e.key === 'Escape') setOpen(false);
    else if (e.key === 'ArrowDown') { e.preventDefault(); setActiveIdx((i) => enabledIdx(Math.min(i + 1, local.options.length - 1), 1) ?? i); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActiveIdx((i) => enabledIdx(Math.max(i - 1, 0), -1) ?? i); }
    else if (e.key === 'Home') { e.preventDefault(); setActiveIdx(enabledIdx(0, 1)); }
    else if (e.key === 'End') { e.preventDefault(); setActiveIdx(enabledIdx(local.options.length - 1, -1)); }
    else if (e.key === 'Enter') { e.preventDefault(); commit(activeIdx()); }
  };
  createEffect(() => {
    if (!open()) return;
    const onDown = (e) => { if (!root.contains(e.target)) setOpen(false); };
    document.addEventListener('pointerdown', onDown);
    onCleanup(() => document.removeEventListener('pointerdown', onDown));
  });

  return (
    <div class="ffield">
      <Show when={local.label}>
        <span class="ffield-label">{local.label}</span>
      </Show>
      <div class="fselect" classList={{ 'is-open': open() }} ref={root}>
        <button type="button" class="fselect-btn" aria-haspopup="listbox" aria-expanded={open()}
                onClick={() => (open() ? setOpen(false) : openAt())} onKeyDown={onKeyDown} {...rest}>
          <span class="fselect-value" classList={{ 'is-placeholder': !selected() }}>
            {selected()?.label ?? local.placeholder ?? 'Select…'}
          </span>
          <ChevronDown />
        </button>
        <Show when={open()}>
          <div class="fselect-pop" role="listbox">
            <For each={local.options}>
              {(opt, i) => (
                <div class="fselect-opt" role="option" aria-selected={opt.value === local.value}
                     classList={{
                       'is-active': i() === activeIdx(),
                       'is-selected': opt.value === local.value,
                       'is-disabled': !!opt.disabled,
                     }}
                     onPointerEnter={() => !opt.disabled && setActiveIdx(i())}
                     onClick={() => commit(i())}>
                  {opt.label}
                  <Show when={opt.value === local.value}>
                    <span class="fselect-check"><CheckMark /></span>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
      <Show when={local.help}>
        <span class="ffield-help" classList={{ 'is-error': !!local.error }}>{local.help}</span>
      </Show>
    </div>
  );
}

/* ---------------- ListBox --------------------------------------------------- */
/* Single-select: value/onChange(value). Multi: multiple + values/onChange(values). */
export function ListBox(props) {
  const [local, rest] = splitProps(props,
    ['options', 'value', 'values', 'onChange', 'multiple', 'label']);
  const [activeIdx, setActiveIdx] = createSignal(-1);

  const isSelected = (opt) =>
    local.multiple ? (local.values ?? []).includes(opt.value) : opt.value === local.value;
  const pick = (opt) => {
    if (opt.disabled) return;
    if (local.multiple) {
      const cur = local.values ?? [];
      local.onChange?.(cur.includes(opt.value) ? cur.filter((v) => v !== opt.value) : [...cur, opt.value]);
    } else {
      local.onChange?.(opt.value);
    }
  };
  const onKeyDown = (e) => {
    const opts = local.options ?? [];
    if (e.key === 'ArrowDown') { e.preventDefault(); setActiveIdx((i) => Math.min(i + 1, opts.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActiveIdx((i) => Math.max(i - 1, 0)); }
    else if (e.key === 'Home') { e.preventDefault(); setActiveIdx(0); }
    else if (e.key === 'End') { e.preventDefault(); setActiveIdx(opts.length - 1); }
    else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (opts[activeIdx()]) pick(opts[activeIdx()]); }
  };

  return (
    <div class="ffield">
      <Show when={local.label}>
        <span class="ffield-label">{local.label}</span>
      </Show>
      <div class="flistbox" role="listbox" tabindex="0" aria-multiselectable={!!local.multiple}
           onKeyDown={onKeyDown} {...rest}>
        <For each={local.options}>
          {(opt, i) => (
            <div class="flistbox-opt" role="option" aria-selected={isSelected(opt)}
                 classList={{ 'is-selected': isSelected(opt), 'is-active': i() === activeIdx(), 'is-disabled': !!opt.disabled }}
                 onClick={() => { setActiveIdx(i()); pick(opt); }}>
              <Show when={local.multiple}>
                <span class="flistbox-check"><CheckMark /></span>
              </Show>
              {opt.label}
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

/* ---------------- Progress -------------------------------------------------- */
/* Thin bar — the default Forge loading treatment. */
export function Progress(props) {
  const merged = mergeProps({ tone: 'accent', value: 0 }, props);
  return (
    <div class="fprogress" classList={{ 'is-indeterminate': !!merged.indeterminate }}>
      <Show when={merged.label || merged.showValue}>
        <div class="fprogress-head">
          <span>{merged.label}</span>
          <Show when={merged.showValue && !merged.indeterminate}>
            <span class="fprogress-value">{Math.round(merged.value)} %</span>
          </Show>
        </div>
      </Show>
      <div class="fprogress-track" role="progressbar"
           aria-valuemin="0" aria-valuemax="100"
           aria-valuenow={merged.indeterminate ? undefined : Math.round(merged.value)}
           aria-label={merged.label}>
        <div class={`fprogress-fill${merged.tone !== 'accent' ? ` tone-${merged.tone}` : ''}`}
             style={merged.indeterminate ? undefined : { width: `${Math.min(100, Math.max(0, merged.value))}%` }} />
      </div>
    </div>
  );
}

/* ---------------- Spinner --------------------------------------------------- */
/* For inline/button-adjacent waits — thin Progress stays the default (tokens.md). */
export function Spinner(props) {
  const merged = mergeProps({ size: 16, label: 'Loading' }, props);
  return (
    <svg class="fspinner" width={merged.size} height={merged.size} viewBox="0 0 24 24"
         fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"
         role="status" aria-label={merged.label}>
      <circle cx="12" cy="12" r="9" opacity="0.25" />
      <path d="M12 3 a 9 9 0 0 1 9 9" />
    </svg>
  );
}

/* ===========================================================================
   Overlays, navigation, structure and extended form controls.
   Anchored popovers position in-place (no Portal) like Select — they clip
   inside scroll containers; see reference/solidjs.md for the caveat.
   =========================================================================== */

const ChevronLeftSvg = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M15 18l-6-6 6-6" />
  </svg>
);
const ChevronRightSvg = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M9 6l6 6-6 6" />
  </svg>
);
const SearchSvg = () => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
    <circle cx="11" cy="11" r="7" /><line x1="21" y1="21" x2="16.5" y2="16.5" />
  </svg>
);
const CalendarSvg = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
    <rect x="3" y="5" width="18" height="16" rx="2" /><line x1="3" y1="10" x2="21" y2="10" />
    <line x1="8" y1="3" x2="8" y2="7" /><line x1="16" y1="3" x2="16" y2="7" />
  </svg>
);

/* Shared dismiss wiring for anchored popovers: click-outside + Escape. */
function useDismiss(open, close, root) {
  createEffect(() => {
    if (!open()) return;
    const onDown = (e) => { if (!root().contains(e.target)) close(); };
    const onKey = (e) => { if (e.key === 'Escape') close(); };
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onKey);
    onCleanup(() => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onKey);
    });
  });
}

/* Shared menu body for DropdownMenu / ContextMenu. */
function MenuList(props) {
  return (
    <For each={props.items}>
      {(item, i) => (
        <Show when={!item.separator} fallback={<div class="fmenu-sep" role="separator" />}>
          <button type="button" class="fmenu-item" role="menuitem" disabled={item.disabled}
                  classList={{
                    'is-active': i() === props.activeIdx(),
                    'is-danger': !!item.danger,
                    'is-disabled': !!item.disabled,
                  }}
                  onPointerEnter={() => !item.disabled && props.setActiveIdx(i())}
                  onClick={() => !item.disabled && props.onCommit(i())}>
            <Show when={item.icon}>
              <Icon of={item.icon} size={14} />
            </Show>
            <span class="fmenu-label">{item.label}</span>
            <Show when={item.kbd}>
              <span class="fmenu-kbd">{item.kbd}</span>
            </Show>
          </button>
        </Show>
      )}
    </For>
  );
}

/* ---------------- Separator ------------------------------------------------- */
export function Separator(props) {
  return <div class="fsep" classList={{ 'is-vertical': !!props.vertical }} role="separator" />;
}

/* ---------------- Skeleton -------------------------------------------------- */
export function Skeleton(props) {
  const [local, rest] = splitProps(props, ['width', 'height', 'style']);
  return (
    <div class="fskel" aria-hidden="true"
         style={{ width: local.width, height: local.height, ...local.style }} {...rest} />
  );
}

/* ---------------- Alert ----------------------------------------------------- */
export function Alert(props) {
  const merged = mergeProps({ tone: 'info' }, props);
  return (
    <div class={`falert falert-${merged.tone}`} role="alert">
      <Show when={merged.icon}>
        <Icon of={merged.icon} size={15} />
      </Show>
      <div>
        <Show when={merged.title}>
          <div class="falert-title">{merged.title}</div>
        </Show>
        <div class="falert-body">{merged.children}</div>
      </div>
    </div>
  );
}

/* ---------------- Avatar ---------------------------------------------------- */
export function Avatar(props) {
  const merged = mergeProps({ size: 'md' }, props);
  const initials = () => (merged.name ?? '')
    .split(/\s+/).slice(0, 2).map((w) => w[0] ?? '').join('').toUpperCase();
  return (
    <span class={`favatar favatar-${merged.size}`} title={merged.name}>
      <Show when={merged.src} fallback={initials()}>
        <img src={merged.src} alt={merged.name} />
      </Show>
      <Show when={merged.status}>
        <span class={`favatar-status fdot-${merged.status}`} style={{ background: `var(--${merged.status === 'neutral' ? 'fg-3' : merged.status})` }} />
      </Show>
    </span>
  );
}

/* ---------------- Tabs (bar only — content is the consumer's Show/Switch) --- */
export function Tabs(props) {
  return (
    <div class="ftabs" role="tablist">
      <For each={props.tabs}>
        {(t) => (
          <button type="button" class="ftab" role="tab" disabled={t.disabled}
                  aria-selected={props.active === t.id}
                  classList={{ 'is-active': props.active === t.id }}
                  onClick={() => props.onChange?.(t.id)}>
            {t.label}
            <Show when={t.count != null}>
              <span class="count">{t.count}</span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}

/* ---------------- ToggleGroup (segmented) ----------------------------------- */
export function ToggleGroup(props) {
  return (
    <div class="fseg" role="radiogroup">
      <For each={props.options}>
        {(opt) => (
          <button type="button" class="fseg-btn" role="radio" disabled={opt.disabled}
                  aria-checked={props.value === opt.value}
                  classList={{ 'is-active': props.value === opt.value }}
                  onClick={() => props.onChange?.(opt.value)}>
            <Show when={opt.icon}>
              <Icon of={opt.icon} size={13} />
            </Show>
            {opt.label}
          </button>
        )}
      </For>
    </div>
  );
}

/* ---------------- Pagination ------------------------------------------------ */
export function Pagination(props) {
  const window_ = () => {
    const { page, pages } = props;
    if (pages <= 7) return Array.from({ length: pages }, (_, i) => i + 1);
    const items = [1];
    if (page > 3) items.push('…');
    for (let p = Math.max(2, page - 1); p <= Math.min(pages - 1, page + 1); p++) items.push(p);
    if (page < pages - 2) items.push('…');
    items.push(pages);
    return items;
  };
  return (
    <nav class="fpage" aria-label="Pagination">
      <button type="button" class="fpage-btn" aria-label="Previous page"
              disabled={props.page <= 1} onClick={() => props.onChange?.(props.page - 1)}>
        <ChevronLeftSvg />
      </button>
      <For each={window_()}>
        {(p) => (
          <Show when={p !== '…'} fallback={<span class="fpage-gap">…</span>}>
            <button type="button" class="fpage-btn"
                    classList={{ 'is-active': p === props.page }}
                    aria-current={p === props.page ? 'page' : undefined}
                    onClick={() => props.onChange?.(p)}>
              {p}
            </button>
          </Show>
        )}
      </For>
      <button type="button" class="fpage-btn" aria-label="Next page"
              disabled={props.page >= props.pages} onClick={() => props.onChange?.(props.page + 1)}>
        <ChevronRightSvg />
      </button>
    </nav>
  );
}

/* ---------------- Collapsible & Accordion ----------------------------------- */
export function Collapsible(props) {
  const [open, setOpen] = createSignal(!!props.defaultOpen);
  const toggle = () => { setOpen((o) => !o); props.onToggle?.(open()); };
  return (
    <div class="facc">
      <div class="facc-item" classList={{ 'is-open': open() }}>
        <button type="button" class="facc-head" aria-expanded={open()} onClick={toggle}>
          {props.title}
          <ChevronDown />
        </button>
        <Show when={open()}>
          <div class="facc-body">{props.children}</div>
        </Show>
      </div>
    </div>
  );
}

export function Accordion(props) {
  const [openId, setOpenId] = createSignal(props.defaultOpen ?? null);
  return (
    <div class="facc">
      <For each={props.items}>
        {(item) => (
          <div class="facc-item" classList={{ 'is-open': openId() === item.id }}>
            <button type="button" class="facc-head" aria-expanded={openId() === item.id}
                    onClick={() => setOpenId((cur) => (cur === item.id ? null : item.id))}>
              {item.title}
              <ChevronDown />
            </button>
            <Show when={openId() === item.id}>
              <div class="facc-body">{item.content}</div>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}

/* ---------------- Textarea -------------------------------------------------- */
export function Textarea(props) {
  const [local, rest] = splitProps(props, ['label', 'help', 'error']);
  return (
    <label class="ffield">
      <Show when={local.label}>
        <span class="ffield-label">{local.label}</span>
      </Show>
      <span class="ffield-area" classList={{ 'is-error': !!local.error }}>
        <textarea {...rest} />
      </span>
      <Show when={local.help}>
        <span class="ffield-help" classList={{ 'is-error': !!local.error }}>{local.help}</span>
      </Show>
    </label>
  );
}

/* ---------------- Slider ---------------------------------------------------- */
export function Slider(props) {
  const merged = mergeProps({ min: 0, max: 100, step: 1 }, props);
  const [local, rest] = splitProps(merged, ['value', 'onChange', 'label', 'showValue', 'min', 'max', 'step']);
  const pct = () => ((local.value - local.min) / (local.max - local.min)) * 100;
  return (
    <div class="ffield">
      <Show when={local.label || local.showValue}>
        <div class="fslider-head">
          <span class="ffield-label">{local.label}</span>
          <Show when={local.showValue}>
            <span class="fslider-value">{local.value}</span>
          </Show>
        </div>
      </Show>
      <input type="range" class="fslider" min={local.min} max={local.max} step={local.step}
             value={local.value} style={{ '--fslider-fill': `${pct()}%` }}
             onInput={(e) => local.onChange?.(Number(e.currentTarget.value))} {...rest} />
    </div>
  );
}

/* ---------------- Tooltip (CSS-only, text-only) ------------------------------ */
export function Tooltip(props) {
  const merged = mergeProps({ side: 'top' }, props);
  return (
    <span class="ftip" data-tip={merged.label} data-side={merged.side}>
      {merged.children}
    </span>
  );
}

/* ---------------- Popover --------------------------------------------------- */
export function Popover(props) {
  const merged = mergeProps({ variant: 'secondary', size: 'md', align: 'start' }, props);
  const [open, setOpen] = createSignal(false);
  let root;
  useDismiss(open, () => setOpen(false), () => root);
  return (
    <div class="fpopover" ref={root}>
      <Button variant={merged.variant} size={merged.size} icon={merged.icon}
              aria-haspopup="dialog" aria-expanded={open()}
              onClick={() => setOpen((o) => !o)}>
        {merged.label}
      </Button>
      <Show when={open()}>
        <div class="fpop fpopover-pop" classList={{ 'is-end': merged.align === 'end' }}
             style={merged.width ? { width: merged.width } : undefined}>
          {merged.children}
        </div>
      </Show>
    </div>
  );
}

/* ---------------- DropdownMenu ---------------------------------------------- */
export function DropdownMenu(props) {
  const merged = mergeProps({ variant: 'secondary', size: 'md', align: 'start' }, props);
  const [open, setOpen] = createSignal(false);
  const [activeIdx, setActiveIdx] = createSignal(-1);
  let root;
  useDismiss(open, () => setOpen(false), () => root);

  const selectable = (i) => {
    const it = merged.items[i];
    return it && !it.separator && !it.disabled;
  };
  const move = (dir) => {
    const n = merged.items.length;
    let i = activeIdx();
    for (let step = 0; step < n; step++) {
      i = (i + dir + n) % n;
      if (selectable(i)) { setActiveIdx(i); return; }
    }
  };
  const commit = (i) => {
    if (!selectable(i)) return;
    setOpen(false);
    merged.items[i].onSelect?.();
  };
  const onKeyDown = (e) => {
    if (!open()) {
      if (['ArrowDown', 'Enter', ' '].includes(e.key)) { e.preventDefault(); setActiveIdx(-1); setOpen(true); move(1); }
      return;
    }
    if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Home') { e.preventDefault(); setActiveIdx(-1); move(1); }
    else if (e.key === 'End') { e.preventDefault(); setActiveIdx(merged.items.length); move(-1); }
    else if (e.key === 'Enter') { e.preventDefault(); commit(activeIdx()); }
  };

  return (
    <div class="fmenu" ref={root}>
      <Button variant={merged.variant} size={merged.size} icon={merged.icon}
              aria-haspopup="menu" aria-expanded={open()}
              onClick={() => setOpen((o) => !o)} onKeyDown={onKeyDown}>
        {merged.label}
      </Button>
      <Show when={open()}>
        <div class="fpop fmenu-pop" role="menu" classList={{ 'is-end': merged.align === 'end' }}>
          <MenuList items={merged.items} activeIdx={activeIdx} setActiveIdx={setActiveIdx} onCommit={commit} />
        </div>
      </Show>
    </div>
  );
}

/* ---------------- ContextMenu ----------------------------------------------- */
/* Wrap any surface; right-click opens the menu at the cursor. */
export function ContextMenu(props) {
  const [pos, setPos] = createSignal(null);  // {x, y} | null
  const [activeIdx, setActiveIdx] = createSignal(-1);
  let root;
  useDismiss(() => !!pos(), () => setPos(null), () => root);
  const commit = (i) => {
    const it = props.items[i];
    if (!it || it.separator || it.disabled) return;
    setPos(null);
    it.onSelect?.();
  };
  return (
    <div class="fctx" ref={root}
         onContextMenu={(e) => {
           e.preventDefault();
           const r = root.getBoundingClientRect();
           setActiveIdx(-1);
           setPos({ x: e.clientX - r.left, y: e.clientY - r.top });
         }}>
      {props.children}
      <Show when={pos()}>
        <div class="fpop fmenu-pop" role="menu"
             style={{ top: `${pos().y}px`, left: `${pos().x}px` }}>
          <MenuList items={props.items} activeIdx={activeIdx} setActiveIdx={setActiveIdx} onCommit={commit} />
        </div>
      </Show>
    </div>
  );
}

/* ---------------- Combobox (searchable select) ------------------------------ */
export function Combobox(props) {
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal(null);  // null = show selected label
  const [activeIdx, setActiveIdx] = createSignal(-1);
  let root, input;
  useDismiss(open, () => { setOpen(false); setQuery(null); }, () => root);

  const selected = () => props.options?.find((o) => o.value === props.value);
  const filtered = () => {
    const q = (query() ?? '').toLowerCase();
    const opts = props.options ?? [];
    return q ? opts.filter((o) => o.label.toLowerCase().includes(q)) : opts;
  };
  const commit = (opt) => {
    if (!opt || opt.disabled) return;
    props.onChange?.(opt.value);
    setOpen(false);
    setQuery(null);
  };
  const onKeyDown = (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setOpen(true); setActiveIdx((i) => Math.min(i + 1, filtered().length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActiveIdx((i) => Math.max(i - 1, 0)); }
    else if (e.key === 'Enter') { e.preventDefault(); commit(filtered()[activeIdx()]); }
    else if (e.key === 'Escape') { setOpen(false); setQuery(null); }
  };

  return (
    <div class="ffield">
      <Show when={props.label}>
        <span class="ffield-label">{props.label}</span>
      </Show>
      <div class="fcombo" ref={root}>
        <span class="ffield-input" classList={{ 'is-error': !!props.error }}>
          <SearchSvg />
          <input ref={input} role="combobox" aria-expanded={open()} disabled={props.disabled}
                 placeholder={props.placeholder}
                 value={query() ?? selected()?.label ?? ''}
                 onInput={(e) => { setQuery(e.currentTarget.value); setOpen(true); setActiveIdx(0); }}
                 onFocus={() => { setOpen(true); input.select(); }}
                 onKeyDown={onKeyDown} />
          <ChevronDown />
        </span>
        <Show when={open()}>
          <div class="fselect-pop" role="listbox">
            <Show when={filtered().length} fallback={<div class="fcmd-empty">{props.emptyText ?? 'No matches'}</div>}>
              <For each={filtered()}>
                {(opt, i) => (
                  <div class="fselect-opt" role="option" aria-selected={opt.value === props.value}
                       classList={{
                         'is-active': i() === activeIdx(),
                         'is-selected': opt.value === props.value,
                         'is-disabled': !!opt.disabled,
                       }}
                       onPointerEnter={() => !opt.disabled && setActiveIdx(i())}
                       onPointerDown={(e) => e.preventDefault()}
                       onClick={() => commit(opt)}>
                    {opt.label}
                    <Show when={opt.value === props.value}>
                      <span class="fselect-check"><CheckMark /></span>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </Show>
      </div>
      <Show when={props.help}>
        <span class="ffield-help" classList={{ 'is-error': !!props.error }}>{props.help}</span>
      </Show>
    </div>
  );
}

/* ---------------- Command palette ------------------------------------------- */
/* Controlled. Bind the hotkey consumer-side:
   (e.metaKey || e.ctrlKey) && e.key === 'k'  ->  open. */
export function Command(props) {
  const [query, setQuery] = createSignal('');
  const [activeIdx, setActiveIdx] = createSignal(0);
  let input;

  const filtered = () => {
    const q = query().toLowerCase();
    return (props.items ?? []).filter((it) => it.label.toLowerCase().includes(q));
  };
  const grouped = () => {
    const out = [];
    for (const it of filtered()) {
      let g = out.find((x) => x.group === (it.group ?? ''));
      if (!g) out.push((g = { group: it.group ?? '', items: [] }));
      g.items.push(it);
    }
    return out;
  };
  const flatIndex = (item) => filtered().indexOf(item);
  const commit = (item) => {
    if (!item) return;
    props.onClose?.();
    setQuery('');
    item.onSelect?.();
  };
  const onKeyDown = (e) => {
    const n = filtered().length;
    if (e.key === 'Escape') { props.onClose?.(); setQuery(''); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); setActiveIdx((i) => (i + 1) % Math.max(1, n)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActiveIdx((i) => (i - 1 + n) % Math.max(1, n)); }
    else if (e.key === 'Enter') { e.preventDefault(); commit(filtered()[activeIdx()]); }
  };
  createEffect(() => { if (props.open) { setActiveIdx(0); queueMicrotask(() => input?.focus()); } });

  return (
    <Show when={props.open}>
      <Portal>
        <div class="fcmd" onClick={(e) => { if (e.target === e.currentTarget) { props.onClose?.(); setQuery(''); } }}>
          <div class="fcmd-panel" role="dialog" aria-label="Command palette">
            <div class="fcmd-input">
              <SearchSvg />
              <input ref={input} placeholder={props.placeholder ?? 'Type a command…'}
                     value={query()}
                     onInput={(e) => { setQuery(e.currentTarget.value); setActiveIdx(0); }}
                     onKeyDown={onKeyDown} />
              <Kbd>esc</Kbd>
            </div>
            <div class="fcmd-list">
              <Show when={filtered().length} fallback={<div class="fcmd-empty">No results</div>}>
                <For each={grouped()}>
                  {(g) => (
                    <>
                      <Show when={g.group}>
                        <div class="fcmd-group">{g.group}</div>
                      </Show>
                      <For each={g.items}>
                        {(item) => (
                          <button type="button" class="fcmd-item"
                                  classList={{ 'is-active': flatIndex(item) === activeIdx() }}
                                  onPointerEnter={() => setActiveIdx(flatIndex(item))}
                                  onClick={() => commit(item)}>
                            <Show when={item.icon}>
                              <Icon of={item.icon} size={14} />
                            </Show>
                            <span class="fmenu-label">{item.label}</span>
                            <Show when={item.kbd}>
                              <span class="fmenu-kbd">{item.kbd}</span>
                            </Show>
                          </button>
                        )}
                      </For>
                    </>
                  )}
                </For>
              </Show>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}

/* ---------------- Sheet (slide-in side panel) -------------------------------- */
export function Sheet(props) {
  createEffect(() => {
    if (!props.open) return;
    const onKey = (e) => { if (e.key === 'Escape') props.onClose?.(); };
    document.addEventListener('keydown', onKey);
    onCleanup(() => document.removeEventListener('keydown', onKey));
  });
  return (
    <Show when={props.open}>
      <Portal>
        <div class="fsheet" classList={{ 'is-left': props.side === 'left' }}>
          <div class="fsheet-scrim" onClick={() => props.onClose?.()} />
          <div class="fsheet-panel" role="dialog" aria-label={props.title}>
            <header class="fsheet-head">
              <h3>{props.title}</h3>
              <button class="ftopbar-icon-btn" aria-label="Close" onClick={() => props.onClose?.()}>
                <XSvg />
              </button>
            </header>
            <div class="fsheet-body">{props.children}</div>
            <Show when={props.footer}>
              <footer class="fsheet-foot">{props.footer}</footer>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}

/* ---------------- Toaster --------------------------------------------------- */
/* Mount <Toaster /> once at the app root, then call toast() from anywhere. */
const [toastItems, setToastItems] = createSignal([]);
let toastSeq = 0;

export function toast(message, opts) {
  const id = ++toastSeq;
  setToastItems((ts) => [...ts, { id, message, tone: opts?.tone ?? 'info', icon: opts?.icon }]);
  const dur = opts?.duration ?? 4000;
  if (dur > 0) setTimeout(() => dismissToast(id), dur);
  return id;
}
export function dismissToast(id) {
  setToastItems((ts) => ts.filter((t) => t.id !== id));
}
export function Toaster() {
  return (
    <Portal>
      <div class="ftoaster">
        <For each={toastItems()}>
          {(t) => (
            <div class={`ftoast ftoast-${t.tone}`} role="status">
              <Show when={t.icon}>
                <Icon of={t.icon} size={14} />
              </Show>
              <span>{t.message}</span>
              <button class="ftoast-close" aria-label="Dismiss" onClick={() => dismissToast(t.id)}>
                <XSvg />
              </button>
            </div>
          )}
        </For>
      </div>
    </Portal>
  );
}

/* ---------------- Calendar & DatePicker ------------------------------------- */
/* Dates are ISO YYYY-MM-DD strings; weeks start Monday; min/max compare
   lexicographically (safe for ISO strings). */
const isoOf = (y, m, d) =>
  `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December'];

export function Calendar(props) {
  const initial = () => {
    const v = props.value ? new Date(`${props.value}T00:00:00`) : new Date();
    return { y: v.getFullYear(), m: v.getMonth() };
  };
  const [view, setView] = createSignal(initial());

  const cells = () => {
    const { y, m } = view();
    const first = new Date(y, m, 1);
    const lead = (first.getDay() + 6) % 7;  // Monday-start offset
    const start = new Date(y, m, 1 - lead);
    return Array.from({ length: 42 }, (_, i) => {
      const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
      return { iso: isoOf(d.getFullYear(), d.getMonth(), d.getDate()), out: d.getMonth() !== m, day: d.getDate() };
    });
  };
  const nav = (dir) => setView(({ y, m }) => {
    const d = new Date(y, m + dir, 1);
    return { y: d.getFullYear(), m: d.getMonth() };
  });
  const today = isoOf(new Date().getFullYear(), new Date().getMonth(), new Date().getDate());
  const disabled = (iso) => (props.min && iso < props.min) || (props.max && iso > props.max);

  return (
    <div class="fcal">
      <div class="fcal-head">
        <button type="button" class="fcal-nav" aria-label="Previous month" onClick={() => nav(-1)}>
          <ChevronLeftSvg />
        </button>
        <span class="fcal-title">{MONTHS[view().m]} {view().y}</span>
        <button type="button" class="fcal-nav" aria-label="Next month" onClick={() => nav(1)}>
          <ChevronRightSvg />
        </button>
      </div>
      <div class="fcal-dow">
        <For each={['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su']}>{(d) => <span>{d}</span>}</For>
      </div>
      <div class="fcal-grid">
        <For each={cells()}>
          {(c) => (
            <button type="button" class="fcal-day" disabled={disabled(c.iso)}
                    classList={{ 'is-out': c.out, 'is-today': c.iso === today, 'is-selected': c.iso === props.value }}
                    onClick={() => props.onChange?.(c.iso)}>
              {c.day}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

export function DatePicker(props) {
  const [open, setOpen] = createSignal(false);
  let root;
  useDismiss(open, () => setOpen(false), () => root);
  return (
    <div class="ffield">
      <Show when={props.label}>
        <span class="ffield-label">{props.label}</span>
      </Show>
      <div class="fdate" ref={root}>
        <button type="button" class="fselect-btn" disabled={props.disabled}
                aria-haspopup="dialog" aria-expanded={open()}
                onClick={() => setOpen((o) => !o)}>
          <CalendarSvg />
          <span class="fselect-value" classList={{ 'is-placeholder': !props.value }}>
            {props.value ?? props.placeholder ?? 'Pick a date…'}
          </span>
          <ChevronDown />
        </button>
        <Show when={open()}>
          <div class="fpop fdate-pop">
            <Calendar value={props.value} min={props.min} max={props.max}
                      onChange={(iso) => { props.onChange?.(iso); setOpen(false); }} />
          </div>
        </Show>
      </div>
      <Show when={props.help}>
        <span class="ffield-help" classList={{ 'is-error': !!props.error }}>{props.help}</span>
      </Show>
    </div>
  );
}

/* ---------------- SplitPane ------------------------------------------------- */
export function SplitPane(props) {
  const merged = mergeProps({ initial: 280, min: 160 }, props);
  const [size, setSize] = createSignal(merged.initial);
  const [dragging, setDragging] = createSignal(false);
  let root;

  const clamp = (px) => {
    const total = merged.vertical ? root.clientHeight : root.clientWidth;
    return Math.min(Math.max(px, merged.min), Math.max(merged.min, total - merged.min - 4));
  };
  const apply = (px) => {
    const v = clamp(px);
    setSize(v);
    merged.onResize?.(v);
  };
  const dividerDown = (e) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setDragging(true);
  };
  const dividerMove = (e) => {
    if (!dragging()) return;
    const r = root.getBoundingClientRect();
    apply(merged.vertical ? e.clientY - r.top : e.clientX - r.left);
  };
  const onKeyDown = (e) => {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') { e.preventDefault(); apply(size() - 16); }
    else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); apply(size() + 16); }
  };

  return (
    <div class={`fsplit ${merged.class ?? ''}`} classList={{ 'is-vertical': !!merged.vertical }}
         style={merged.style} ref={root}>
      <div class="fsplit-pane" style={merged.vertical ? { height: `${size()}px`, flex: 'none' } : { width: `${size()}px`, flex: 'none' }}>
        {merged.first}
      </div>
      <div class="fsplit-divider" role="separator" tabindex="0"
           aria-orientation={merged.vertical ? 'horizontal' : 'vertical'}
           classList={{ 'is-dragging': dragging() }}
           onPointerDown={dividerDown} onPointerMove={dividerMove}
           onPointerUp={() => setDragging(false)} onKeyDown={onKeyDown} />
      <div class="fsplit-pane" style={{ flex: 1 }}>{merged.second}</div>
    </div>
  );
}
