// No state and no DOM, so it can be tested on its own.
import type { RecentEntry } from '../../ipc';
import type { Env } from '../../ui/types';
import { buildRows, folderChain, type TreeItems } from './tree';

export type RecentRow = {
  id: string;
  name: string;
  detail: string;
  protocol: string;
  env: Env;
  path: string;
  opened: number;
};

export function recentRows(entries: RecentEntry[], items: TreeItems): RecentRow[] {
  const rows = buildRows(items, new Set(items.folders.map((f) => f.id)));
  return entries.flatMap(({ id, opened }) => {
    const row = rows.find((r) => r.item.kind === 'connection' && r.item.id === id);
    const connection = items.connections.find((c) => c.id === id);
    if (!row || !connection) return [];
    const folders = folderChain(connection.folder, items.folders).reverse();
    const path = [...folders.map((f) => f.name), row.name].join(' / ');
    return [
      {
        id,
        name: row.name,
        detail: row.detail,
        protocol: connection.protocol,
        env: row.env,
        path,
        opened,
      },
    ];
  });
}

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// `opened` and `now` are seconds since the Unix epoch.
export function openedAgo(opened: number, now: number): string {
  const ago = now - opened;
  if (ago < MINUTE) return 'Just now';
  if (ago < HOUR) return `${Math.floor(ago / MINUTE)} min ago`;
  if (ago < DAY) return `${Math.floor(ago / HOUR)} h ago`;
  if (ago < 2 * DAY) return 'Yesterday';
  if (ago < 7 * DAY) return `${Math.floor(ago / DAY)} days ago`;
  const date = new Date(opened * 1000);
  const sameYear = date.getFullYear() === new Date(now * 1000).getFullYear();
  return date.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
}

export function openedAt(opened: number): string {
  return new Date(opened * 1000).toLocaleString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
