import { commands } from '../../bindings';
import type { RecentEntry } from '../../ipc';
import { recentRows } from './recent';
import { saved } from './saved.svelte';

const FOLDED_SETTING = 'ela.recent.folded';
const LIMIT = 8;
const nowSeconds = () => Math.floor(Date.now() / 1000);

class Recent {
  entries = $state<RecentEntry[]>([]);
  loadError = $state('');
  folded = $state(localStorage.getItem(FOLDED_SETTING) === 'true');
  // Ticks so "5 min ago" moves on while the list is on screen.
  now = $state(nowSeconds());

  rows = $derived(
    recentRows(this.entries, { connections: saved.connections, folders: saved.folders }),
  );

  constructor() {
    setInterval(() => (this.now = nowSeconds()), 30_000);
    saved.onOpened = (id) => this.opened(id);
  }

  async load() {
    try {
      this.entries = await commands.listRecent();
      this.loadError = '';
    } catch (e) {
      this.loadError = String(e);
    }
  }

  // Rust records it as the Session starts; this only saves asking for the list again.
  opened(id: string) {
    this.now = nowSeconds();
    this.entries = [
      { id, opened: this.now },
      ...this.entries.filter((entry) => entry.id !== id),
    ].slice(0, LIMIT);
  }

  setFolded(folded: boolean) {
    this.folded = folded;
    localStorage.setItem(FOLDED_SETTING, String(folded));
  }

  async forget(id: string) {
    try {
      await commands.forgetRecent(id);
      this.entries = this.entries.filter((entry) => entry.id !== id);
    } catch (e) {
      saved.showToast({
        message: "Couldn't remove it from Recent",
        detail: String(e),
        error: true,
      });
    }
  }

  async clear() {
    try {
      await commands.clearRecent();
      this.entries = [];
    } catch (e) {
      saved.showToast({ message: "Couldn't clear Recent", detail: String(e), error: true });
    }
  }
}

export const recent = new Recent();
