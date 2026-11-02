import { describe, expect, test } from 'bun:test';
import { clampPanelWidth, PANEL_MAX, PANEL_MIN, readPanelWidths } from './panelWidth';

describe('clampPanelWidth', () => {
  test('stays within the limits, in whole pixels', () => {
    expect(clampPanelWidth(10)).toBe(PANEL_MIN);
    expect(clampPanelWidth(9000)).toBe(PANEL_MAX);
    expect(clampPanelWidth(300.4)).toBe(300);
  });
});

describe('readPanelWidths', () => {
  test('keeps known sections and clamps them', () => {
    expect(readPanelWidths('{"files":9000,"vault":250,"other":300,"knownhosts":"x"}')).toEqual({
      files: PANEL_MAX,
      vault: 250,
    });
  });

  test('anything unreadable counts as nothing chosen', () => {
    expect(readPanelWidths(null)).toEqual({});
    expect(readPanelWidths('not json')).toEqual({});
    expect(readPanelWidths('3')).toEqual({});
  });
});
