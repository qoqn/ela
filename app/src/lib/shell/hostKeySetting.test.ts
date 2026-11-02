import { describe, expect, test } from 'bun:test';
import type { HostKeyCode } from '../ipc';
import { LEGACY_HOST_KEY, loadHostKey } from './hostKeySetting';

function storage(legacy: string | null) {
  const items = new Map<string, string>();
  if (legacy !== null) items.set(LEGACY_HOST_KEY, legacy);
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    removeItem: (key: string) => void items.delete(key),
  };
}

function backend(saved: HostKeyCode | null, { failSave = false } = {}) {
  const writes: HostKeyCode[] = [];
  return {
    writes,
    hostKey: async () => saved,
    setHostKey: async (code: HostKeyCode) => {
      if (failSave) throw "Can't write settings.toml";
      writes.push(code);
      saved = code;
      return null;
    },
  };
}

describe('loadHostKey', () => {
  test('nothing chosen anywhere leaves the default to the caller', async () => {
    const local = storage(null);
    const settings = backend(null);
    expect(await loadHostKey(local, settings)).toBeNull();
    expect(settings.writes).toEqual([]);
  });

  test('a choice in localStorage moves to settings.toml once', async () => {
    const local = storage('AltRight');
    const settings = backend(null);
    expect(await loadHostKey(local, settings)).toBe('AltRight');
    expect(settings.writes).toEqual(['AltRight']);
    expect(local.items.size).toBe(0);

    expect(await loadHostKey(local, settings)).toBe('AltRight');
    expect(settings.writes).toEqual(['AltRight']);
  });

  test('settings.toml wins over an old value, which is dropped', async () => {
    const local = storage('AltRight');
    const settings = backend('Pause');
    expect(await loadHostKey(local, settings)).toBe('Pause');
    expect(settings.writes).toEqual([]);
    expect(local.items.size).toBe(0);
  });

  test('an old value that is no host key is dropped, not moved', async () => {
    const local = storage('KeyA');
    const settings = backend(null);
    expect(await loadHostKey(local, settings)).toBeNull();
    expect(settings.writes).toEqual([]);
    expect(local.items.size).toBe(0);
  });

  test('the old value stays when it could not be moved', async () => {
    const local = storage('AltRight');
    await expect(loadHostKey(local, backend(null, { failSave: true }))).rejects.toBe(
      "Can't write settings.toml",
    );
    expect(local.getItem(LEGACY_HOST_KEY)).toBe('AltRight');
  });
});
