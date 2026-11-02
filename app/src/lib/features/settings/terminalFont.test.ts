import { describe, expect, test } from 'bun:test';
import { clampSize, fontChoices, fontStack, MAX_SIZE, MIN_SIZE } from './terminalFont';

describe('clampSize', () => {
  test('stays within the limits, in whole pixels', () => {
    expect(clampSize(1)).toBe(MIN_SIZE);
    expect(clampSize(99)).toBe(MAX_SIZE);
    expect(clampSize(14.4)).toBe(14);
  });
});

describe('fontStack', () => {
  test('the built-in font alone when nothing is chosen', () => {
    expect(fontStack(null)).toBe("'IBM Plex Mono', ui-monospace, monospace");
  });

  test('a chosen font comes first, and a quote in its name cannot break out', () => {
    expect(fontStack("Bob's Mono")).toBe("'Bobs Mono', 'IBM Plex Mono', ui-monospace, monospace");
  });
});

describe('fontChoices', () => {
  const labels = (choices: { label: string }[]) => choices.map((choice) => choice.label);

  test('the built-in font first and once, then the system fonts by name', () => {
    expect(labels(fontChoices(['Fira 10', 'Fira 2', 'IBM Plex Mono'], null, true))).toEqual([
      'IBM Plex Mono (built in)',
      'Fira 2',
      'Fira 10',
    ]);
  });

  test('a saved font that is gone stays visible and says so', () => {
    expect(labels(fontChoices(['Fira'], 'Gone Mono', true))).toEqual([
      'IBM Plex Mono (built in)',
      'Gone Mono (not installed)',
      'Fira',
    ]);
  });

  test('a saved font is not called missing before the list has loaded', () => {
    expect(labels(fontChoices([], 'Fira', false))).toEqual(['IBM Plex Mono (built in)', 'Fira']);
  });
});
