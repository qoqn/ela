const UNITS = ['KB', 'MB', 'GB', 'TB'];
const DAY_S = 86400;

export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  let value = bytes;
  let unit = -1;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${UNITS[unit]}`;
}

export function formatModified(seconds: number | null, now = Date.now() / 1000): string {
  if (seconds === null) return '';
  const age = now - seconds;
  if (age >= 0 && age < 3600) return `${Math.max(1, Math.round(age / 60))} min ago`;
  if (age >= 0 && age < DAY_S) return `${Math.round(age / 3600)} h ago`;
  if (age >= 0 && age < 7 * DAY_S) return `${Math.round(age / DAY_S)} d ago`;
  const date = new Date(seconds * 1000);
  const month = date.toLocaleString('en-US', { month: 'short' });
  return `${date.getDate()} ${month}${age > 300 * DAY_S ? ` ${date.getFullYear()}` : ''}`;
}

export function percent(done: number, total: number): number {
  return total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
}

export function deleteWording(
  entry: { name: string; kind: 'folder' | 'file' | 'other'; link: boolean },
  host: string,
): { title: string; body: string } {
  if (entry.link) {
    return {
      title: 'Delete this link?',
      body: `Only the link ${entry.name} is removed from ${host}. What it points to stays.`,
    };
  }
  if (entry.kind === 'folder') {
    return {
      title: 'Delete this folder?',
      body: `${entry.name} and everything in it are removed from ${host}. This can't be undone.`,
    };
  }
  return {
    title: 'Delete this file?',
    body: `${entry.name} is removed from ${host}. This can't be undone.`,
  };
}
