// No state and no DOM, so it can be tested on its own.
import { compareNames } from '../../names';
import type { Env } from '../../ui/types';
import { hasTag, parseFilter, type FilterTerms } from './tags';
import type {
  ItemRef,
  Outside,
  SavedConnection,
  SavedFolder,
  SavedSettings,
  TrashEntry,
} from '../../ipc';

export type TreeItems = { connections: SavedConnection[]; folders: SavedFolder[] };

export type Row = {
  key: string;
  item: ItemRef;
  name: string;
  detail: string;
  protocol?: string;
  tags: string[];
  level: number;
  parent: string | null;
  expandable: boolean;
  expanded: boolean;
  env: Env;
};

export const rowKey = (item: ItemRef) => `${item.kind}:${item.id}`;

const byName = (a: { name: string }, b: { name: string }) => compareNames(a.name, b.name);

/** The Folders from `start` upwards, stopping at a missing parent or a loop like the Rust side does. */
export function folderChain(
  start: string | null | undefined,
  folders: SavedFolder[],
): SavedFolder[] {
  const chain: SavedFolder[] = [];
  let id = start;
  while (id) {
    const folder = folders.find((f) => f.id === id);
    if (!folder || chain.includes(folder)) break;
    chain.push(folder);
    id = folder.parent;
  }
  return chain;
}

/**
 * What a new item in `start` would inherit, for showing in the form. Rust stays authoritative.
 * `outside` is the layer below every Folder, such as what `~/.ssh/config` says about the host.
 */
export function inheritedSettings(
  start: string | null | undefined,
  folders: SavedFolder[],
  outside: Outside | null = null,
) {
  const chain = folderChain(start, folders);
  const below = outside?.settings ?? {};
  const first = <K extends keyof SavedSettings>(field: K) =>
    chain.find((f) => f[field] != null)?.[field] ?? below[field];
  const options: Record<string, unknown> = { ...(below.options ?? {}) };
  for (const folder of [...chain].reverse()) Object.assign(options, folder.options);
  return {
    port: first('port'),
    username: first('username'),
    credential: first('credential'),
    environment: first('environment'),
    options,
    folderName: (field: keyof SavedSettings) =>
      chain.find((f) => f[field] != null)?.name ??
      (below[field] != null ? outside?.source : undefined),
  };
}

function environmentOf(
  own: SavedSettings,
  parent: string | null | undefined,
  folders: SavedFolder[],
): Env {
  return own.environment ?? inheritedSettings(parent, folders).environment ?? 'none';
}

function connectionMatches(connection: SavedConnection, { tags, words }: FilterTerms) {
  const text = [
    connection.name,
    connection.host,
    connection.username ?? '',
    ...(connection.tags ?? []),
  ].map((value) => value.toLowerCase());
  return (
    tags.every((tag) => hasTag(connection.tags, tag)) &&
    words.every((word) => text.some((value) => value.includes(word)))
  );
}

function folderMatches(folder: SavedFolder, { tags, words }: FilterTerms) {
  const name = folder.name.toLowerCase();
  return !tags.length && words.every((word) => name.includes(word));
}

export function buildRows(
  { connections, folders }: TreeItems,
  expanded: ReadonlySet<string>,
  filterText = '',
): Row[] {
  const terms = parseFilter(filterText);
  const filter = terms.tags.length + terms.words.length > 0;
  const childFolders = new Map<string | null, SavedFolder[]>();
  const childConnections = new Map<string | null, SavedConnection[]>();
  const known = new Set(folders.map((f) => f.id));
  const push = <T>(map: Map<string | null, T[]>, key: string | null, value: T) =>
    map.set(key, [...(map.get(key) ?? []), value]);

  for (const folder of folders)
    push(childFolders, folder.parent && known.has(folder.parent) ? folder.parent : null, folder);
  for (const connection of connections) {
    push(
      childConnections,
      connection.folder && known.has(connection.folder) ? connection.folder : null,
      connection,
    );
  }

  const visited = new Set<string>();

  function visit(
    parent: string | null,
    level: number,
    includeAll: boolean,
    parentKey: string | null,
  ): Row[] {
    const rows: Row[] = [];
    for (const folder of [...(childFolders.get(parent) ?? [])].sort(byName)) {
      if (visited.has(folder.id)) continue;
      rows.push(...folderRows(folder, level, includeAll, parentKey));
    }
    for (const connection of [...(childConnections.get(parent) ?? [])].sort(byName)) {
      if (filter && !includeAll && !connectionMatches(connection, terms)) continue;
      const item: ItemRef = { kind: 'connection', id: connection.id };
      rows.push({
        key: rowKey(item),
        item,
        name: connection.name,
        detail: connection.host,
        protocol: connection.protocol,
        tags: connection.tags ?? [],
        level,
        parent: parentKey,
        expandable: false,
        expanded: false,
        env: environmentOf(connection, connection.folder, folders),
      });
    }
    return rows;
  }

  function folderRows(
    folder: SavedFolder,
    level: number,
    includeAll: boolean,
    parentKey: string | null,
  ): Row[] {
    visited.add(folder.id);
    const item: ItemRef = { kind: 'folder', id: folder.id };
    const key = rowKey(item);
    const selfMatches = !filter || includeAll || folderMatches(folder, terms);
    const children = visit(folder.id, level + 1, filter && selfMatches, key);
    if (filter && !selfMatches && !children.length) return [];
    const open = filter ? true : expanded.has(folder.id);
    const row: Row = {
      key,
      item,
      name: folder.name,
      detail: '',
      tags: [],
      level,
      parent: parentKey,
      expandable: true,
      expanded: open,
      env: environmentOf(folder, folder.parent, folders),
    };
    return open ? [row, ...children] : [row];
  }

  const rows = visit(null, 0, false, null);
  // Folders caught in a loop have no way in from the top. They stay reachable as top-level rows.
  for (const folder of [...folders].sort(byName)) {
    if (!visited.has(folder.id)) rows.push(...folderRows(folder, 0, false, null));
  }
  return rows;
}

export function folderAndInside(id: string, folders: SavedFolder[]): Set<string> {
  const found = new Set([id]);
  for (let grew = true; grew;) {
    grew = false;
    for (const folder of folders) {
      if (folder.parent && found.has(folder.parent) && !found.has(folder.id)) {
        found.add(folder.id);
        grew = true;
      }
    }
  }
  return found;
}

export function canMoveTo(
  item: ItemRef,
  to: string | null,
  { connections, folders }: TreeItems,
): boolean {
  if (item.kind === 'connection') {
    return (connections.find((c) => c.id === item.id)?.folder ?? null) !== to;
  }
  if (to && folderAndInside(item.id, folders).has(to)) return false;
  return (folders.find((f) => f.id === item.id)?.parent ?? null) !== to;
}

export type TreeStep = { active?: string; expand?: string; collapse?: string };

export function treeStep(key: string, rows: Row[], active: string | null): TreeStep | null {
  if (!rows.length) return null;
  const index = rows.findIndex((row) => row.key === active);
  const row = rows[index];
  switch (key) {
    case 'ArrowDown':
      return { active: rows[Math.min(index + 1, rows.length - 1)].key };
    case 'ArrowUp':
      return { active: rows[index < 0 ? 0 : Math.max(index - 1, 0)].key };
    case 'Home':
      return { active: rows[0].key };
    case 'End':
      return { active: rows[rows.length - 1].key };
    case 'ArrowRight':
      if (!row?.expandable) return {};
      if (!row.expanded) return { expand: row.item.id };
      return rows[index + 1]?.parent === row.key ? { active: rows[index + 1].key } : {};
    case 'ArrowLeft':
      if (row?.expandable && row.expanded) return { collapse: row.item.id };
      return row?.parent ? { active: row.parent } : {};
    default:
      return null;
  }
}

export function parsePort(text: string): { port: number | undefined } | { error: string } {
  const value = text.trim();
  if (!value) return { port: undefined };
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535)
    return { error: 'A port is a number from 1 to 65535.' };
  return { port };
}

export type OptionField = {
  type?: string;
  values?: string[];
  labels?: Record<string, string>;
  default?: unknown;
  label?: string;
  hint?: string;
  group?: 'sign_in' | 'route' | 'advanced';
  mono?: boolean;
};

export type OptionEntry = [string, OptionField];

export function optionEntries(schema: unknown): OptionEntry[] {
  return Object.entries((schema ?? {}) as Record<string, OptionField>);
}

export type ProtocolOptions = { id: string; name: string; fields: OptionEntry[] };

// A Folder serves every Protocol, so a key two Protocols share, like the Jump Host, is one field.
export function groupOptions(protocols: { id: string; name: string; schema: unknown }[]) {
  const seen = new Set<string>();
  const signIn: OptionEntry[] = [];
  const route: OptionEntry[] = [];
  const advanced: OptionEntry[] = [];
  const shared = { sign_in: signIn, route, advanced };
  const own: ProtocolOptions[] = [];
  for (const protocol of protocols) {
    const fields: OptionEntry[] = [];
    for (const entry of optionEntries(protocol.schema)) {
      const [key, field] = entry;
      if (!field.group) fields.push(entry);
      else if (!seen.has(key)) shared[field.group].push(entry);
      seen.add(key);
    }
    if (fields.length) own.push({ id: protocol.id, name: protocol.name, fields });
  }
  return { signIn, route, advanced, own };
}

/** One entry per line. Lists were comma-separated text before, which this still reads. */
export function listLines(value: unknown): string {
  const entries = Array.isArray(value) ? value.map(String) : String(value ?? '').split(',');
  return entries
    .map((entry) => entry.trim())
    .filter(Boolean)
    .join('\n');
}

export function optionsAsText(
  options: Record<string, unknown> | undefined,
  listKeys: ReadonlySet<string>,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(options ?? {}).map(([key, value]) => [
      key,
      listKeys.has(key) ? listLines(value) : String(value),
    ]),
  );
}

export function listKeysOf(schemas: unknown[]): Set<string> {
  return new Set(
    schemas.flatMap((schema) =>
      optionEntries(schema)
        .filter(([, field]) => field.type === 'list')
        .map(([key]) => key),
    ),
  );
}

export function optionsToSave(
  existing: Record<string, unknown> | undefined,
  knownKeys: ReadonlySet<string>,
  fields: [string, OptionField][],
  text: Record<string, string>,
): Record<string, unknown> {
  // Keys no known Protocol describes came from elsewhere, so they stay.
  const options = Object.fromEntries(
    Object.entries(existing ?? {}).filter(([key]) => !knownKeys.has(key)),
  );
  for (const [key, field] of fields) {
    const value = text[key]?.trim();
    if (!value) continue;
    if (field.type === 'list') {
      options[key] = value
        .split('\n')
        .map((entry) => entry.trim())
        .filter(Boolean);
      continue;
    }
    options[key] = field.type === 'integer' && /^-?\d+$/.test(value) ? Number(value) : value;
  }
  return options;
}

const counted = (count: number, one: string, many: string) =>
  `${count} ${count === 1 ? one : many}`;

export function trashSize(items: TrashEntry[]): number {
  return items.reduce((sum, entry) => sum + 1 + entry.inside.length, 0);
}

export function trashSummary(items: TrashEntry[]): string {
  const all = items.flatMap((entry) => [entry.item, ...entry.inside]);
  const folders = all.filter((item) => item.kind === 'folder').length;
  const parts = [
    [all.length - folders, 'connection', 'connections'],
    [folders, 'folder', 'folders'],
  ] as const;
  return parts
    .filter(([count]) => count)
    .map(([count, one, many]) => counted(count, one, many))
    .join(' and ');
}

export function trashDetail(entry: TrashEntry): string {
  if (entry.host != null) return entry.host;
  const inside = entry.inside.length;
  return inside ? `Folder, ${counted(inside, 'item', 'items')} inside` : 'Folder';
}
