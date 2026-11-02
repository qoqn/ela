import { isTauri } from '@tauri-apps/api/core';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';

export async function copyText(text: string): Promise<boolean> {
  if (isTauri()) {
    try {
      await writeText(text);
      return true;
    } catch (e) {
      console.warn('could not write the clipboard', e);
      return false;
    }
  }
  // The browser preview has no plugin.
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export async function pasteText(): Promise<string | null> {
  try {
    // The browser preview has no sessions to sync with, and shouldn't ask for permission.
    return isTauri() ? await readText() : null;
  } catch {
    return null;
  }
}
