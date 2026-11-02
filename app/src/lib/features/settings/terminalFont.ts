import { compareNames } from '../../names';

export const BUILT_IN_FONT = 'IBM Plex Mono';
export const DEFAULT_SIZE = 13;
export const MIN_SIZE = 8;
export const MAX_SIZE = 32;

export const clampSize = (size: number) => Math.round(Math.min(MAX_SIZE, Math.max(MIN_SIZE, size)));

// The built-in font stays in the stack for what a chosen font has no glyph for.
export function fontStack(family: string | null): string {
  const chosen = family ? `'${family.replace(/['\\]/g, '')}', ` : '';
  return `${chosen}'${BUILT_IN_FONT}', ui-monospace, monospace`;
}

export function fontChoices(system: string[], family: string | null, loaded: boolean) {
  const others = system.filter((name) => name !== BUILT_IN_FONT).sort(compareNames);
  const missing = family && loaded && !others.includes(family);
  return [
    { value: '', label: `${BUILT_IN_FONT} (built in)` },
    ...(family && (missing || !loaded)
      ? [{ value: family, label: missing ? `${family} (not installed)` : family }]
      : []),
    ...others.map((name) => ({ value: name, label: name })),
  ];
}
