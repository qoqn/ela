/// <reference types="bun" />
import { describe, expect, test } from 'bun:test';
import { fuzzyScore, looksLikeAddress } from './palette';

describe('fuzzyScore', () => {
  test('an empty query matches everything equally', () => {
    expect(fuzzyScore('', 'Close Tab')).toBe(0);
    expect(fuzzyScore('  ', 'Fullscreen')).toBe(0);
  });

  test('letters must appear in order', () => {
    expect(fuzzyScore('clt', 'Close Tab')).not.toBeNull();
    expect(fuzzyScore('tlc', 'Close Tab')).toBeNull();
    expect(fuzzyScore('xyz', 'Close Tab')).toBeNull();
  });

  test('case does not matter', () => {
    expect(fuzzyScore('CLOSE', 'close tab')).toBe(fuzzyScore('close', 'Close Tab'));
  });

  test('a run of letters beats scattered ones', () => {
    expect(fuzzyScore('tab', 'Close Tab')!).toBeGreaterThan(fuzzyScore('tab', 'Toggle a sidebar')!);
  });

  test('word starts beat the middle of a word', () => {
    expect(fuzzyScore('s', 'Shortcut sheet')!).toBeGreaterThan(fuzzyScore('s', 'Close')!);
  });
});

describe('looksLikeAddress', () => {
  test('addresses', () => {
    for (const text of [
      'user@host',
      'root@10.0.0.5:22',
      'rdp://win-ad',
      'ssh://a@b:2222',
      'db-01.internal',
      '10.0.0.5',
      'host.lan:2222',
    ]) {
      expect(looksLikeAddress(text)).toBe(true);
    }
  });

  test('not addresses', () => {
    for (const text of ['', 'close', 'close tab', 'toggle sidebar', 'a@', 'rdp://']) {
      expect(looksLikeAddress(text)).toBe(false);
    }
  });
});
