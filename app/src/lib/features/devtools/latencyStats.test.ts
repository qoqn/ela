import { describe, expect, test } from 'bun:test';
import { inversions, markValue, MARK_WIDTH, percentile, summarise } from './latencyStats';

function row(value: number) {
  const rgba = new Uint8ClampedArray(MARK_WIDTH * 4);
  for (let x = 0; x < MARK_WIDTH; x++) {
    if ((value >> Math.floor(x / 32)) & 1) rgba[x * 4] = 255;
  }
  return rgba;
}

describe('latency statistics', () => {
  test('a mark reads back every value, even with JPEG noise', () => {
    for (const value of [0, 1, 2, 85, 170, 255]) {
      const noisy = row(value).map((v) => (v ? v - 30 : v + 30));
      expect(markValue(noisy)).toBe(value);
    }
  });

  test('percentiles take a value that was measured', () => {
    const sorted = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(sorted, 0.5)).toBe(51);
    expect(percentile(sorted, 0.99)).toBe(100);
    expect(percentile([], 0.5)).toBeNaN();
  });

  test('a stage only some samples have is summarised from those', () => {
    const summary = summarise([{ a: 3, b: 7 }, { a: 1 }, { a: 2 }]);
    expect(summary['a.p50']).toBe(2);
    expect(summary['b.p50']).toBe(7);
  });

  test('inversions count every step backwards', () => {
    expect(inversions([1, 2, 3])).toBe(0);
    expect(inversions([1, 3, 2, 4, 0])).toBe(2);
  });
});
