import { describe, expect, test } from 'bun:test';
import { compareNames } from './names';

describe('compareNames', () => {
  test('numbers in a name count as numbers', () => {
    expect(['web-10', 'web-2', 'web-1'].sort(compareNames)).toEqual(['web-1', 'web-2', 'web-10']);
  });
});
