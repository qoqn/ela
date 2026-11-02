import { describe, expect, test } from 'bun:test';
import { linkText, summariseLinks } from './link';

describe('summariseLinks', () => {
  test('nothing open has no link', () => {
    expect(summariseLinks([])).toBeUndefined();
  });

  test('the state that needs the user wins and every Session counts', () => {
    expect(summariseLinks(['connected', 'failed', 'closed'])).toEqual({
      state: 'failed',
      count: 3,
    });
    expect(summariseLinks(['closed', 'connected'])?.state).toBe('connected');
  });
});

describe('linkText', () => {
  test('names the count only when there are several', () => {
    expect(linkText({ state: 'connected', count: 1 })).toBe('connected');
    expect(linkText({ state: 'connected', count: 2 })).toBe('connected, 2 sessions');
  });
});
