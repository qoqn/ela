import type { KnownHost, KnownHostSection } from '../../ipc';
import { compareNames } from '../../names';

export type HostRow = { kind: 'host'; key: string; section: KnownHostSection; entry: KnownHost };
export type Row = HostRow | HeadingRow;

function readableName(entry: KnownHost) {
  return entry.hashed ? entry.known_as : entry.name;
}

function byName(a: KnownHost, b: KnownHost) {
  const [nameA, nameB] = [readableName(a), readableName(b)];
  return (
    Number(nameA === null) - Number(nameB === null) ||
    compareNames(nameA ?? a.name, nameB ?? b.name)
  );
}

function matches(entry: KnownHost, filter: string) {
  // The text of a hashed name is meaningless, and matching it would only look like a result.
  return (
    !!readableName(entry)?.toLowerCase().includes(filter) ||
    entry.fingerprint.toLowerCase().includes(filter)
  );
}

export type HeadingRow = {
  kind: 'heading';
  key: string;
  section: KnownHostSection;
  count: number;
  open: boolean;
};

// A filter opens every store, so a match is never hidden behind a closed heading.
export function knownHostRows(
  sections: KnownHostSection[],
  filter: string,
  closed: ReadonlySet<string> = new Set(),
): Row[] {
  const wanted = filter.trim().toLowerCase();
  return sections.flatMap((section) => {
    const entries = section.entries.filter((entry) => matches(entry, wanted)).sort(byName);
    if (wanted && !entries.length && !section.problem) return [];
    const open = !!wanted || !closed.has(section.store);
    const heading: Row = {
      kind: 'heading',
      key: section.store,
      section,
      count: entries.length,
      open,
    };
    if (!open) return [heading];
    return [
      heading,
      ...entries.map((entry): Row => ({
        kind: 'host',
        key: `${section.store} ${entry.name} ${entry.fingerprint}`,
        section,
        entry,
      })),
    ];
  });
}

export function shownName(entry: KnownHost) {
  return readableName(entry) ?? 'Hashed name';
}
