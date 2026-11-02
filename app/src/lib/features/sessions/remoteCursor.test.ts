import { describe, expect, test } from 'bun:test';
import { cursorPlacement } from './remoteCursor';

const image = (size: number, hot = 0) => ({
  shape: 'image' as const,
  width: size,
  height: size,
  hot_x: hot,
  hot_y: hot,
  rgba: [] as number[],
});

describe('remote cursor placement', () => {
  test('a remote pixel stays a screen pixel on a HiDPI screen', () => {
    expect(cursorPlacement(image(48, 8), 0.5, 2)).toEqual({
      pixelWidth: 48,
      pixelHeight: 48,
      hotX: 4,
      hotY: 4,
    });
  });

  test('the cursor scales with a fitted view, hotspot included', () => {
    expect(cursorPlacement(image(32, 10), 0.5, 1)).toEqual({
      pixelWidth: 16,
      pixelHeight: 16,
      hotX: 5,
      hotY: 5,
    });
  });

  test('a very large cursor stops at the limit, keeping its proportions', () => {
    const wide = { ...image(384, 192), height: 192 };
    expect(cursorPlacement(wide, 1, 1)).toEqual({
      pixelWidth: 128,
      pixelHeight: 64,
      hotX: 64,
      hotY: 64,
    });
  });

  test('a view not laid out yet leaves the size alone', () => {
    expect(cursorPlacement(image(32), 0, 1).pixelWidth).toBe(32);
  });
});
