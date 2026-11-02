import { compareNames } from '../../names';

export type FilterTerms = { tags: string[]; words: string[] };

export const cleanTag = (raw: string) => raw.trim().replace(/^#+/, '').replace(/\s+/g, '-');

// A filter term ends at a space, so a Tag saved with one in it is searched with a dash instead.
const tagTerm = (tag: string) => cleanTag(tag).toLowerCase();

export function parseFilter(text: string): FilterTerms {
  const terms = text.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return {
    tags: terms.filter((term) => term.length > 1 && term.startsWith('#')).map(tagTerm),
    words: terms.filter((term) => !term.startsWith('#')),
  };
}

export const hasTag = (tags: string[] | null | undefined, wanted: string) =>
  (tags ?? []).some((tag) => tagTerm(tag).startsWith(wanted));

export function addTags(tags: string[], raw: string): string[] {
  const next = [...tags];
  for (const tag of raw.split(',').map(cleanTag).filter(Boolean)) {
    if (!next.some((present) => tagTerm(present) === tagTerm(tag))) next.push(tag);
  }
  return next;
}

export function allTags(connections: { tags?: string[] | null }[]): string[] {
  const found = new Map<string, string>();
  for (const tag of connections.flatMap((connection) => connection.tags ?? [])) {
    if (!found.has(tagTerm(tag))) found.set(tagTerm(tag), cleanTag(tag));
  }
  return [...found.values()].sort(compareNames);
}

export const filtersByTag = (filter: string, tag: string) =>
  parseFilter(filter).tags.includes(tagTerm(tag));

export function toggleTagFilter(filter: string, tag: string): string {
  const term = `#${tagTerm(tag)}`;
  const terms = filter.trim().split(/\s+/).filter(Boolean);
  const without = terms.filter((other) => other.toLowerCase() !== term);
  return (without.length === terms.length ? [...terms, term] : without).join(' ');
}

export type SearchIntent = 'idle' | 'search' | 'address';

export function searchIntent(text: string, matches: number): SearchIntent {
  const value = text.trim();
  if (!value) return 'idle';
  if (value.startsWith('#') || /\s/.test(value)) return 'search';
  if (/[@:]/.test(value)) return 'address';
  return matches ? 'search' : 'address';
}
