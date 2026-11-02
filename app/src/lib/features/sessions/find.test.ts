import { describe, expect, test } from 'bun:test';
import { findPatternError, findStatus } from './find';

describe('findStatus', () => {
  test('says nothing before anything is typed', () => {
    expect(findStatus('', { index: -1, count: 0 }, 1000)).toBe('');
  });

  test('counts matches and says which one is selected', () => {
    expect(findStatus('pg', { index: -1, count: 0 }, 1000)).toBe('No results');
    expect(findStatus('pg', { index: 2, count: 12 }, 1000)).toBe('3 of 12');
    expect(findStatus('pg', { index: -1, count: 1 }, 1000)).toBe('1 result');
    expect(findStatus('pg', { index: -1, count: 12 }, 1000)).toBe('12 results');
  });

  test('says when there are more matches than are counted', () => {
    expect(findStatus('e', { index: 4, count: 1000 }, 1000)).toBe('5 of 1,000+');
    expect(findStatus('e', { index: -1, count: 1000 }, 1000)).toBe('1,000+ results');
  });
});

test('only a regular expression can be invalid', () => {
  expect(findPatternError('a(b', true)).toBe('Not a valid pattern');
  expect(findPatternError('a(b', false)).toBe('');
  expect(findPatternError('a(b)', true)).toBe('');
});
