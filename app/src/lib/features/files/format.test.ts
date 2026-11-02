import { describe, expect, test } from 'bun:test';
import { deleteWording, formatModified, formatSize, percent } from './format';

describe('formatSize', () => {
  test('follows the units in the copy guide', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(999)).toBe('999 B');
    expect(formatSize(1400)).toBe('1.4 KB');
    expect(formatSize(1_400_000)).toBe('1.4 MB');
    expect(formatSize(23_000_000)).toBe('23 MB');
    expect(formatSize(5_000_000_000_000_000)).toBe('5000 TB');
  });
});

describe('formatModified', () => {
  const now = 1_800_000_000;
  test('is relative under seven days', () => {
    expect(formatModified(now - 5, now)).toBe('1 min ago');
    expect(formatModified(now - 600, now)).toBe('10 min ago');
    expect(formatModified(now - 7200, now)).toBe('2 h ago');
    expect(formatModified(now - 3 * 86400, now)).toBe('3 d ago');
  });

  test('is absolute after that, and for clocks ahead of ours', () => {
    expect(formatModified(now - 30 * 86400, now)).toMatch(/^\d+ [A-Z][a-z]{2}$/);
    expect(formatModified(now - 800 * 86400, now)).toMatch(/^\d+ [A-Z][a-z]{2} \d{4}$/);
    expect(formatModified(now + 86400, now)).toMatch(/^\d+ [A-Z][a-z]{2}/);
  });

  test('says nothing when the server does not', () => {
    expect(formatModified(null, now)).toBe('');
  });

  test('names months the same way in every engine', () => {
    const september = Date.UTC(2026, 8, 15, 12) / 1000;
    expect(formatModified(september, september + 30 * 86400)).toBe('15 Sep');
  });
});

describe('percent', () => {
  test('stays in range when a size is unknown or grew', () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(5, 0)).toBe(0);
    expect(percent(300, 200)).toBe(100);
  });
});

describe('deleteWording', () => {
  const entry = (kind: 'folder' | 'file' | 'other', link = false) => ({
    name: 'logs',
    kind,
    link,
  });

  test('warns that a folder goes with its contents', () => {
    expect(deleteWording(entry('folder'), 'db-01')).toEqual({
      title: 'Delete this folder?',
      body: "logs and everything in it are removed from db-01. This can't be undone.",
    });
  });

  test('says a link to a folder only removes the link', () => {
    expect(deleteWording(entry('folder', true), 'db-01').title).toBe('Delete this link?');
    expect(deleteWording(entry('other', true), 'db-01').body).toContain('What it points to stays');
  });

  test('names a plain file', () => {
    expect(deleteWording(entry('file'), 'db-01').title).toBe('Delete this file?');
  });
});
