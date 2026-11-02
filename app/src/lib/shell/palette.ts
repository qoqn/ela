// No state and no DOM, so it can be tested on its own.

export function fuzzyScore(query: string, text: string): number | null {
  const needle = query.trim().toLowerCase();
  const haystack = text.toLowerCase();
  let score = 0;
  let from = 0;
  let previous = -2;
  for (const letter of needle) {
    const at = haystack.indexOf(letter, from);
    if (at < 0) return null;
    if (at === previous + 1) score += 3;
    if (at === 0 || /[\s\-_./:@]/.test(haystack[at - 1])) score += 2;
    score -= Math.min(at - from, 3) / 10;
    previous = at;
    from = at + 1;
  }
  return score;
}

export function looksLikeAddress(query: string): boolean {
  const text = query.trim();
  if (/\s/.test(text)) return false;
  return (
    /^\w[\w+-]*:\/\/\S/.test(text) ||
    /^[^@]+@\S/.test(text) ||
    /^[\w-]+(\.[\w-]+)+(:\d+)?$/.test(text)
  );
}
