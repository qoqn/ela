import { describe, expect, test } from 'bun:test';
import { idleLabel } from './idle';

describe('idleLabel', () => {
  test('counts minutes below an hour and hours from there', () => {
    expect(idleLabel(1)).toBe('1 min');
    expect(idleLabel(15)).toBe('15 min');
    expect(idleLabel(60)).toBe('1 hour');
    expect(idleLabel(240)).toBe('4 hours');
  });
});
