import { describe, expect, it } from 'vitest';

import { SURFACES, attachedSurfaces, attachedUnits, surfaceOf, usageLine } from './surfaces';

const cliNode = { kind: 'cli_command', id: 'parse', cells: {} };
const apiNode = { kind: 'code_item', id: 'http', cells: { kind: { text: ':api' } } };
const dbNode = { kind: 'code_item', id: 'schema', cells: { kind: { text: ':db_schema' } } };
const screenNode = { kind: 'screen', id: 'login', cells: {} };

describe('surfaceOf', () => {
  it('matches the curated kinds', () => {
    expect(surfaceOf(cliNode)?.id).toBe('cli');
    expect(surfaceOf(screenNode)?.id).toBe('screen');
  });

  it('gates code_item on its kind cell (bare or symbol form)', () => {
    expect(surfaceOf(apiNode)?.id).toBe('api');
    expect(surfaceOf({ ...apiNode, cells: { kind: { text: 'api' } } })?.id).toBe('api');
    expect(surfaceOf(dbNode)).toBeNull();
  });

  it('leaves other kinds to the generic forms', () => {
    expect(surfaceOf({ kind: 'adr', cells: {} })).toBeNull();
    expect(surfaceOf(null)).toBeNull();
  });
});

describe('attachedUnits / attachedSurfaces', () => {
  const component = { kind: 'component', id: 'cli_core', cells: {} };
  const model = {
    nodes: [
      component,
      { ...cliNode, parents: [{ field: 'component', kind: 'component', id: 'cli_core' }] },
      {
        kind: 'cli_command',
        id: 'check',
        cells: {},
        parents: [{ field: 'component', kind: 'component', id: 'other' }],
      },
      { ...apiNode, parents: [{ field: 'component', kind: 'component', id: 'cli_core' }] },
      { ...dbNode, parents: [{ field: 'component', kind: 'component', id: 'cli_core' }] },
      { ...screenNode, parents: [{ field: 'container', kind: 'container', id: 'cli_core' }] },
    ],
  };

  it('finds units whose parent links name the node, honouring the gate', () => {
    const cli = SURFACES.find((s) => s.id === 'cli');
    expect(attachedUnits(component, model, cli).map((n) => n.id)).toEqual(['parse']);
    const api = SURFACES.find((s) => s.id === 'api');
    // The :db_schema code_item is filtered by the surface gate.
    expect(attachedUnits(component, model, api).map((n) => n.id)).toEqual(['http']);
  });

  it('follows containment transitively — a container aggregates its components commands', () => {
    const container = { kind: 'container', id: 'cli_bin', cells: {} };
    const deep = {
      nodes: [
        container,
        {
          kind: 'component',
          id: 'cli_commands',
          cells: {},
          parents: [{ field: 'container', kind: 'container', id: 'cli_bin' }],
        },
        {
          kind: 'cli_command',
          id: 'parse',
          cells: {},
          parents: [{ field: 'component', kind: 'component', id: 'cli_commands' }],
        },
        {
          kind: 'cli_command',
          id: 'other',
          cells: {},
          parents: [{ field: 'component', kind: 'component', id: 'elsewhere' }],
        },
      ],
    };
    const cli = SURFACES.find((s) => s.id === 'cli');
    expect(attachedUnits(container, deep, cli).map((n) => n.id)).toEqual(['parse']);
    expect(attachedSurfaces(container, deep).map((x) => x.surface.id)).toEqual(['cli']);
  });

  it('offers the surface tab on a kind-marked host even with nothing attached', () => {
    const cliComp = { kind: 'component', id: 'cmds', cells: { kind: { text: ':cli' } } };
    const tuiComp = { kind: 'component', id: 'console', cells: { kind: { text: ':tui' } } };
    const apiComp = { kind: 'component', id: 'httpd', cells: { kind: { text: ':web_api' } } };
    const plain = { kind: 'component', id: 'lexer', cells: { kind: { text: ':module' } } };
    const m = { nodes: [cliComp, tuiComp, apiComp, plain] };
    expect(attachedSurfaces(cliComp, m).map((x) => x.surface.id)).toEqual(['cli']);
    expect(attachedSurfaces(tuiComp, m).map((x) => x.surface.id)).toEqual(['screen']);
    expect(attachedSurfaces(apiComp, m).map((x) => x.surface.id)).toEqual(['api']);
    expect(attachedSurfaces(plain, m)).toEqual([]);
  });

  it('survives containment cycles', () => {
    const a = { kind: 'component', id: 'a', cells: {}, parents: [{ field: 'parent', kind: 'component', id: 'b' }] };
    const b = { kind: 'component', id: 'b', cells: {}, parents: [{ field: 'parent', kind: 'component', id: 'a' }] };
    const cmd = {
      kind: 'cli_command',
      id: 'run',
      cells: {},
      parents: [{ field: 'component', kind: 'component', id: 'b' }],
    };
    const cli = SURFACES.find((s) => s.id === 'cli');
    expect(attachedUnits(a, { nodes: [a, b, cmd] }, cli).map((n) => n.id)).toEqual(['run']);
  });

  it('aggregates only surfaces with units, and never the node own kind', () => {
    const got = attachedSurfaces(component, model);
    expect(got.map((x) => x.surface.id)).toEqual(['cli', 'api', 'screen']);
    // A cli_command aggregates screens/apis it owns, never other commands.
    const selfModel = {
      nodes: [{ ...cliNode, parents: [{ field: 'component', kind: 'component', id: 'parse' }] }],
    };
    expect(attachedSurfaces(cliNode, selfModel)).toEqual([]);
  });
});

describe('usageLine', () => {
  it('renders name, bracketed args and flags', () => {
    const detail = {
      id: 'parse',
      cells: { name: { text: 'wcl parse' } },
      children: [
        {
          kind: 'cli_arg',
          items: [
            { label: 'file', cells: { name: { text: '<file>' } } },
            { label: 'tpl', cells: { name: { text: 'template' }, required: { text: 'false' } } },
          ],
        },
        {
          kind: 'cli_flag',
          items: [
            { label: 'out', cells: { name: { text: '--out' }, value: { text: '<dir>' } } },
            { label: 'quiet', cells: { name: { text: '-q' } } },
          ],
        },
      ],
    };
    expect(usageLine(detail)).toBe('wcl parse <file> [template] [--out <dir>] [-q]');
  });

  it('falls back to the id with no children', () => {
    expect(usageLine({ id: 'parse', cells: {}, children: [] })).toBe('parse');
  });
});
