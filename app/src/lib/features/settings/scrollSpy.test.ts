import { describe, expect, test } from 'bun:test';
import { currentSection } from './scrollSpy';

describe('currentSection', () => {
  const tops = [0, 300, 700];

  test('the first section before anything has scrolled', () => {
    expect(currentSection(tops, 0)).toBe(0);
  });

  test('the last section whose top has reached the line', () => {
    expect(currentSection(tops, 299)).toBe(0);
    expect(currentSection(tops, 300)).toBe(1);
    expect(currentSection(tops, 5000)).toBe(2);
  });
});
