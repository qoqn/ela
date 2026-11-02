import { describe, expect, test } from 'bun:test';
import type { SavedConnection, SavedFolder } from '../../ipc';
import { openedAgo, recentRows } from './recent';

const NOW = Date.UTC(2026, 10, 1, 12, 0) / 1000;

describe('openedAgo', () => {
  test('relative under a week, as the design asks', () => {
    expect(openedAgo(NOW - 20, NOW)).toBe('Just now');
    expect(openedAgo(NOW + 30, NOW)).toBe('Just now');
    expect(openedAgo(NOW - 5 * 60, NOW)).toBe('5 min ago');
    expect(openedAgo(NOW - 3 * 3600 - 59, NOW)).toBe('3 h ago');
    expect(openedAgo(NOW - 30 * 3600, NOW)).toBe('Yesterday');
    expect(openedAgo(NOW - 3 * 86400, NOW)).toBe('3 days ago');
  });

  test('a date after that, with the year only when it differs', () => {
    expect(openedAgo(Date.UTC(2026, 9, 12, 12) / 1000, NOW)).toBe('12 Oct');
    expect(openedAgo(Date.UTC(2025, 9, 12, 12) / 1000, NOW)).toBe('12 Oct 2025');
  });
});

describe('recentRows', () => {
  const prod: SavedFolder = {
    id: 'f1',
    name: 'Production',
    environment: 'prod',
  } as SavedFolder;
  const db: SavedConnection = {
    id: 'c1',
    name: 'db-01',
    folder: 'f1',
    protocol: 'ssh',
    host: 'db-01.internal',
  } as SavedConnection;
  const lab: SavedConnection = {
    id: 'c2',
    name: 'pi',
    protocol: 'ssh',
    host: 'pi.lan',
  } as SavedConnection;

  test('keeps the order it is given, with folder path and inherited environment', () => {
    const rows = recentRows(
      [
        { id: 'c2', opened: 2 },
        { id: 'c1', opened: 1 },
      ],
      { connections: [db, lab], folders: [prod] },
    );
    expect(rows.map((r) => r.name)).toEqual(['pi', 'db-01']);
    expect(rows[1]).toMatchObject({ path: 'Production / db-01', env: 'prod', opened: 1 });
    expect(rows[0].path).toBe('pi');
  });

  test('leaves out what no longer exists', () => {
    const rows = recentRows([{ id: 'gone', opened: 1 }], { connections: [db], folders: [prod] });
    expect(rows).toEqual([]);
  });
});
