import { describe, expect, test } from 'bun:test';
import type { SavedConnection, SavedFolder } from '../../ipc';
import {
  buildRows,
  canMoveTo,
  folderAndInside,
  groupOptions,
  inheritedSettings,
  listKeysOf,
  listLines,
  optionsAsText,
  optionsToSave,
  parsePort,
  trashDetail,
  trashSize,
  trashSummary,
  treeStep,
} from './tree';

const folder = (id: string, parent?: string, more: Partial<SavedFolder> = {}): SavedFolder => ({
  id,
  name: id,
  parent,
  ...more,
});
const connection = (
  id: string,
  parent?: string,
  more: Partial<SavedConnection> = {},
): SavedConnection => ({
  id,
  name: id,
  folder: parent,
  protocol: 'ssh',
  host: `${id}.lan`,
  ...more,
});

const items = {
  folders: [
    folder('prod', undefined, { environment: 'prod', username: 'root', port: 2200 }),
    folder('db', 'prod'),
  ],
  connections: [connection('db-1', 'db'), connection('web', 'prod'), connection('alone')],
};
const names = (rows: { name: string }[]) => rows.map((row) => row.name);

describe('buildRows', () => {
  test('folders come first and only expanded ones show their contents', () => {
    expect(names(buildRows(items, new Set()))).toEqual(['prod', 'alone']);
    expect(names(buildRows(items, new Set(['prod'])))).toEqual(['prod', 'db', 'web', 'alone']);
    expect(names(buildRows(items, new Set(['prod', 'db'])))).toEqual([
      'prod',
      'db',
      'db-1',
      'web',
      'alone',
    ]);
  });

  test('rows know their level and parent', () => {
    const rows = buildRows(items, new Set(['prod', 'db']));
    const row = rows.find((r) => r.name === 'db-1')!;
    expect(row.level).toBe(2);
    expect(row.parent).toBe('folder:db');
  });

  test('environment comes from the nearest folder that sets it, and a connection can opt out', () => {
    const rows = buildRows(
      {
        folders: items.folders,
        connections: [connection('a', 'db'), connection('b', 'db', { environment: 'none' })],
      },
      new Set(['prod', 'db']),
    );
    expect(rows.find((r) => r.name === 'a')!.env).toBe('prod');
    expect(rows.find((r) => r.name === 'b')!.env).toBe('none');
  });

  test('a filter keeps matches and opens the folders leading to them', () => {
    const rows = buildRows(items, new Set(), 'DB-1');
    expect(names(rows)).toEqual(['prod', 'db', 'db-1']);
    expect(rows[0].expanded).toBe(true);
  });

  test('a filter looks at the host and tags too', () => {
    const tagged = {
      folders: [],
      connections: [connection('a', undefined, { tags: ['eu-west'] }), connection('b')],
    };
    expect(names(buildRows(tagged, new Set(), 'b.lan'))).toEqual(['b']);
    expect(names(buildRows(tagged, new Set(), 'eu-'))).toEqual(['a']);
  });

  test('a folder that matches keeps all it contains', () => {
    expect(names(buildRows(items, new Set(), 'prod'))).toEqual(['prod', 'db', 'db-1', 'web']);
  });

  test('a filter with no match gives no rows', () => {
    expect(buildRows(items, new Set(['prod']), 'nothing')).toEqual([]);
  });

  test('an item whose parent is gone shows at the top', () => {
    const orphans = { folders: [folder('f', 'gone')], connections: [connection('c', 'gone')] };
    expect(names(buildRows(orphans, new Set()))).toEqual(['f', 'c']);
  });

  test('folders in a loop stay reachable and show once', () => {
    const looped = {
      folders: [folder('a', 'b'), folder('b', 'a')],
      connections: [connection('c', 'a')],
    };
    const rows = buildRows(looped, new Set(['a', 'b']));
    expect(names(rows).filter((name) => name === 'a')).toHaveLength(1);
    expect(names(rows)).toContain('c');
  });
});

describe('inheritedSettings', () => {
  test('takes the nearest value and says where from', () => {
    const inherited = inheritedSettings('db', [
      ...items.folders,
      folder('x', 'db', { username: 'near' }),
    ]);
    expect(inherited.username).toBe('root');
    expect(inherited.folderName('username')).toBe('prod');
    expect(
      inheritedSettings('x', [...items.folders, folder('x', 'db', { username: 'near' })]).username,
    ).toBe('near');
  });

  test('nearer options win key by key', () => {
    const folders = [
      folder('top', undefined, { options: { term: 'vt100', domain: 'CORP' } }),
      folder('near', 'top', { options: { term: 'xterm' } }),
    ];
    expect(inheritedSettings('near', folders).options).toEqual({ term: 'xterm', domain: 'CORP' });
  });
});

describe('moves', () => {
  test('a folder and its contents are off limits for that folder', () => {
    expect(folderAndInside('prod', items.folders)).toEqual(new Set(['prod', 'db']));
    expect(canMoveTo({ kind: 'folder', id: 'prod' }, 'db', items)).toBe(false);
    expect(canMoveTo({ kind: 'folder', id: 'prod' }, 'prod', items)).toBe(false);
  });

  test('moving to where an item already is does not count', () => {
    expect(canMoveTo({ kind: 'connection', id: 'web' }, 'prod', items)).toBe(false);
    expect(canMoveTo({ kind: 'connection', id: 'alone' }, null, items)).toBe(false);
    expect(canMoveTo({ kind: 'folder', id: 'db' }, null, items)).toBe(true);
    expect(canMoveTo({ kind: 'connection', id: 'web' }, 'db', items)).toBe(true);
  });
});

describe('treeStep', () => {
  const rows = buildRows(items, new Set(['prod']));
  const [prod, db, web, alone] = rows.map((row) => row.key);

  test('up and down stop at the ends', () => {
    expect(treeStep('ArrowDown', rows, prod)).toEqual({ active: db });
    expect(treeStep('ArrowDown', rows, alone)).toEqual({ active: alone });
    expect(treeStep('ArrowUp', rows, prod)).toEqual({ active: prod });
    expect(treeStep('ArrowDown', rows, null)).toEqual({ active: prod });
  });

  test('right expands, then enters', () => {
    expect(treeStep('ArrowRight', rows, db)).toEqual({ expand: 'db' });
    expect(treeStep('ArrowRight', rows, prod)).toEqual({ active: db });
    expect(treeStep('ArrowRight', rows, web)).toEqual({});
  });

  test('left collapses, then goes to the parent', () => {
    expect(treeStep('ArrowLeft', rows, prod)).toEqual({ collapse: 'prod' });
    expect(treeStep('ArrowLeft', rows, web)).toEqual({ active: prod });
    expect(treeStep('ArrowLeft', rows, alone)).toEqual({});
  });

  test('other keys are not its business', () => {
    expect(treeStep('a', rows, prod)).toBeNull();
    expect(treeStep('ArrowDown', [], null)).toBeNull();
  });
});

describe('parsePort', () => {
  test('blank means unset', () => expect(parsePort('  ')).toEqual({ port: undefined }));
  test('a real port passes', () => expect(parsePort('2222')).toEqual({ port: 2222 }));
  test.each(['0', '65536', 'ssh', '22.5'])('%s is refused', (text) =>
    expect('error' in parsePort(text)).toBe(true),
  );
});

describe('optionsToSave', () => {
  const fields: [string, { type?: string }][] = [
    ['term', { type: 'string' }],
    ['retries', { type: 'integer' }],
  ];
  const known = new Set(['term', 'retries', 'domain']);

  test('keeps keys nothing describes and drops known ones left empty', () => {
    expect(
      optionsToSave({ custom: 1, term: 'old', domain: 'CORP' }, known, fields, { term: ' ' }),
    ).toEqual({ custom: 1 });
  });

  test('trims text and turns whole numbers into numbers for integer fields', () => {
    expect(optionsToSave(undefined, known, fields, { term: ' screen ', retries: '3' })).toEqual({
      term: 'screen',
      retries: 3,
    });
    expect(optionsToSave(undefined, known, fields, { retries: 'many' })).toEqual({
      retries: 'many',
    });
  });
});

describe('the Trash', () => {
  const ref = (kind: 'connection' | 'folder', id: string) => ({ kind, id });
  const lone = { item: ref('connection', 'a'), name: 'a', host: 'a.lan', inside: [] };
  const empty = { item: ref('folder', 'b'), name: 'b', host: null, inside: [] };
  const full = {
    item: ref('folder', 'c'),
    name: 'c',
    host: null,
    inside: [ref('connection', 'd'), ref('connection', 'e'), ref('folder', 'f')],
  };

  test('counts what is inside folders', () => {
    expect(trashSize([])).toBe(0);
    expect(trashSize([lone, full])).toBe(5);
  });

  test('summarises by kind with the right plural', () => {
    expect(trashSummary([lone])).toBe('1 connection');
    expect(trashSummary([empty])).toBe('1 folder');
    expect(trashSummary([lone, full])).toBe('3 connections and 2 folders');
  });

  test('describes each entry', () => {
    expect(trashDetail(lone)).toBe('a.lan');
    expect(trashDetail(empty)).toBe('Folder');
    expect(trashDetail(full)).toBe('Folder, 3 items inside');
    expect(trashDetail({ ...full, inside: [ref('connection', 'd')] })).toBe(
      'Folder, 1 item inside',
    );
  });
});

describe('groupOptions', () => {
  const jump = { type: 'string', label: 'Jump Host', group: 'route' };
  const grouped = groupOptions([
    {
      id: 'ssh',
      name: 'SSH',
      schema: {
        term: { type: 'string' },
        identity_file: { type: 'string', group: 'sign_in' },
        proxy_jump: jump,
      },
    },
    {
      id: 'rdp',
      name: 'RDP',
      schema: { domain: { type: 'string', group: 'sign_in' }, proxy_jump: jump },
    },
    { id: 'bare', name: 'Bare', schema: {} },
  ]);

  test('a key two protocols share is offered once', () => {
    expect(grouped.route.map(([key]) => key)).toEqual(['proxy_jump']);
    expect(grouped.signIn.map(([key]) => key)).toEqual(['identity_file', 'domain']);
  });

  test('only protocols with options of their own get a section', () => {
    expect(grouped.own.map((p) => [p.id, p.fields.map(([key]) => key)])).toEqual([
      ['ssh', ['term']],
    ]);
  });
});

describe('list options', () => {
  test('a list shows one entry per line, older comma text included', () => {
    expect(listLines(['~/.ssh/a,b', ' /c '])).toBe('~/.ssh/a,b\n/c');
    expect(listLines('~/.ssh/a, /b ,,')).toBe('~/.ssh/a\n/b');
    expect(listLines(undefined)).toBe('');
  });

  test('only keys a schema calls a list are read as one', () => {
    const keys = listKeysOf([
      { identity_file: { type: 'list' }, term: { type: 'string' } },
      { proxy_jump: { type: 'list' } },
    ]);
    expect([...keys].sort()).toEqual(['identity_file', 'proxy_jump']);
    expect(optionsAsText({ identity_file: 'a, b', term: 'x,y' }, keys)).toEqual({
      identity_file: 'a\nb',
      term: 'x,y',
    });
  });

  test('a list is saved as an array, so an entry may hold a comma', () => {
    const fields: [string, { type: string }][] = [['identity_file', { type: 'list' }]];
    const known = new Set(['identity_file']);
    expect(
      optionsToSave(undefined, known, fields, { identity_file: ' ~/.ssh/a,b \n\n/c ' }),
    ).toEqual({ identity_file: ['~/.ssh/a,b', '/c'] });
    expect(optionsToSave(undefined, known, fields, { identity_file: '\n ' })).toEqual({});
  });
});

describe('the outside layer', () => {
  test('fills what no Folder sets, and names where it came from', () => {
    const folder = {
      id: 'f',
      name: 'Databases',
      username: 'postgres',
      options: { term: 'screen' },
    } as SavedFolder;
    const outside = {
      source: '~/.ssh/config',
      hostname: 'db.internal',
      settings: {
        port: 2222,
        username: 'file',
        options: { term: 'xterm', identity_file: ['~/k'] },
      },
    };
    const inherited = inheritedSettings('f', [folder], outside);
    expect(inherited.port).toBe(2222);
    expect(inherited.folderName('port')).toBe('~/.ssh/config');
    expect(inherited.username).toBe('postgres');
    expect(inherited.folderName('username')).toBe('Databases');
    expect(inherited.options).toEqual({ term: 'screen', identity_file: ['~/k'] });
    expect(inheritedSettings('f', [folder]).port).toBeUndefined();
  });
});
