import { workspace, type SessionTab } from '../../sessions.svelte';
import { saved } from '../tree/saved.svelte';

class Reconnect {
  start(tab: SessionTab) {
    if (tab.connection) return void saved.connect(tab.connection, tab.key);
    try {
      workspace.reopen(tab);
    } catch (e) {
      saved.showToast({
        message: `Couldn't reconnect to ${tab.name}`,
        detail: String(e),
        error: true,
      });
    }
  }
}

export const reconnect = new Reconnect();
