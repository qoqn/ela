import { describe, expect, test } from 'bun:test';
import { folderFromOsc7 } from './osc7';

describe('folderFromOsc7', () => {
  test('reads the folder from a file URL with a host', () => {
    expect(folderFromOsc7('file://db-01/var/lib/postgresql')).toBe('/var/lib/postgresql');
  });

  test('reads it without a host', () => {
    expect(folderFromOsc7('file:///home/you')).toBe('/home/you');
  });

  test('decodes escaped characters', () => {
    expect(folderFromOsc7('file://db-01/home/you/My%20Files')).toBe('/home/you/My Files');
  });

  test('ignores anything that is not a file URL', () => {
    expect(folderFromOsc7('/home/you')).toBeNull();
    expect(folderFromOsc7('https://db-01/home')).toBeNull();
    expect(folderFromOsc7('file://db-01/bad%ZZ')).toBeNull();
  });
});
