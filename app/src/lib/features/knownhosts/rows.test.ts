import { describe, expect, test } from 'bun:test';
import type { KnownHost, KnownHostSection } from '../../ipc';
import { knownHostRows, shownName } from './rows';

const host = (
  name: string,
  fingerprint = `SHA256:${name}`,
  hashed = false,
  known_as: string | null = null,
): KnownHost => ({
  name,
  hashed,
  known_as,
  algorithm: 'ssh-ed25519',
  fingerprint,
});

const section = (
  entries: KnownHost[],
  store: KnownHostSection['store'] = 'ssh_keys',
  problem: string | null = null,
): KnownHostSection => ({ store, path: '/k', entries, other_lines: 0, problem });

const names = (sections: KnownHostSection[], filter = '', closed = new Set<string>()) =>
  knownHostRows(sections, filter, closed).map((row) =>
    row.kind === 'host' ? row.entry.name : row.key,
  );

describe('knownHostRows', () => {
  test('sorts by name with hashed names last, under a heading per store', () => {
    expect(
      names([
        section([host('|1|x|y', 'SHA256:h', true), host('zeta'), host('alpha')]),
        section([host('[db]:3389')], 'certificates'),
      ]),
    ).toEqual(['ssh_keys', 'alpha', 'zeta', '|1|x|y', 'certificates', '[db]:3389']);
  });

  test('keeps the heading of an empty store so it can say so', () => {
    expect(names([section([])])).toEqual(['ssh_keys']);
  });

  test('filters by name and fingerprint, ignoring case', () => {
    const sections = [section([host('db-01', 'SHA256:Abc'), host('web', 'SHA256:xyz')])];
    expect(names(sections, 'DB')).toEqual(['ssh_keys', 'db-01']);
    expect(names(sections, 'abc')).toEqual(['ssh_keys', 'db-01']);
  });

  test('a filter never matches the text of a hashed name', () => {
    expect(names([section([host('|1|db|y', 'SHA256:h', true)])], 'db')).toEqual([]);
  });

  test('a hashed name known from a saved Connection sorts and filters by that name', () => {
    const sections = [
      section([
        host('|1|a|b', 'SHA256:h', true),
        host('zeta'),
        host('|1|c|d', 'SHA256:k', true, '[build.lan]:2222'),
        host('alpha'),
      ]),
    ];
    expect(names(sections)).toEqual(['ssh_keys', '|1|c|d', 'alpha', 'zeta', '|1|a|b']);
    expect(names(sections, 'BUILD')).toEqual(['ssh_keys', '|1|c|d']);
    expect(names(sections, '|1|')).toEqual([]);
  });

  test('a filter hides a store with no match, unless it has a problem to show', () => {
    expect(names([section([host('a')])], 'nope')).toEqual([]);
    expect(names([section([], 'ssh_keys', 'broken')], 'nope')).toEqual(['ssh_keys']);
  });

  test('a closed store shows only its heading, with the count', () => {
    const sections = [section([host('a'), host('b')]), section([host('c')], 'certificates')];
    expect(names(sections, '', new Set(['ssh_keys']))).toEqual(['ssh_keys', 'certificates', 'c']);
    const heading = knownHostRows(sections, '', new Set(['ssh_keys']))[0];
    expect(heading).toMatchObject({ kind: 'heading', count: 2, open: false });
  });

  test('a filter opens closed stores so matches show', () => {
    expect(names([section([host('db-01')])], 'db', new Set(['ssh_keys']))).toEqual([
      'ssh_keys',
      'db-01',
    ]);
  });
});

test('shownName never shows the text of a hashed name, only the name it is known as', () => {
  expect(shownName(host('|1|x|y', 'SHA256:h', true))).toBe('Hashed name');
  expect(shownName(host('db-01'))).toBe('db-01');
  expect(shownName(host('|1|x|y', 'SHA256:h', true, 'db-01'))).toBe('db-01');
});
