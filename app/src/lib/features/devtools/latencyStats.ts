export const LATENCY_OSC = 7777;
export const MARK_ROW = 16;
export const MARK_WIDTH = 256;
const MARK_BITS = 8;
const MARK_CELL = 32;

export function markValue(row: ArrayLike<number>): number {
  let value = 0;
  for (let bit = 0; bit < MARK_BITS; bit++) {
    const x = MARK_CELL / 2 + bit * MARK_CELL;
    if (row[x * 4] > 128) value |= 1 << bit;
  }
  return value;
}

export function percentile(sorted: readonly number[], q: number): number {
  if (!sorted.length) return Number.NaN;
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
}

export function summarise(samples: readonly Record<string, number>[]): Record<string, number> {
  const stages = new Set(samples.flatMap((s) => Object.keys(s)));
  const summary: Record<string, number> = {};
  for (const stage of stages) {
    const sorted = samples
      .map((s) => s[stage])
      .filter((v) => v !== undefined)
      .sort((a, b) => a - b);
    summary[`${stage}.p50`] = percentile(sorted, 0.5);
    summary[`${stage}.p95`] = percentile(sorted, 0.95);
    summary[`${stage}.p99`] = percentile(sorted, 0.99);
  }
  return summary;
}

export function inversions(sequence: readonly number[]): number {
  let count = 0;
  for (let i = 1; i < sequence.length; i++) if (sequence[i] < sequence[i - 1]) count++;
  return count;
}
