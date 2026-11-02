export function folderFromOsc7(data: string): string | null {
  let url: URL;
  try {
    url = new URL(data);
  } catch {
    return null;
  }
  if (url.protocol !== 'file:' || !url.pathname.startsWith('/')) return null;
  try {
    return decodeURIComponent(url.pathname);
  } catch {
    return null;
  }
}
