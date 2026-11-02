export type FindResults = { index: number; count: number };

export function findPatternError(query: string, regex: boolean): string {
  if (!regex) return '';
  try {
    new RegExp(query);
    return '';
  } catch {
    return 'Not a valid pattern';
  }
}

// xterm's search stops counting at its highlight limit, and its index is -1 when the
// selected match isn't one of those counted, or when output moved it away.
export function findStatus(query: string, results: FindResults, limit: number): string {
  if (!query) return '';
  const { index, count } = results;
  if (count === 0) return 'No results';
  const total = count >= limit ? `${limit.toLocaleString('en')}+` : String(count);
  if (index >= 0) return `${index + 1} of ${total}`;
  return `${total} ${count === 1 ? 'result' : 'results'}`;
}
