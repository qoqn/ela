import { commands } from '../../bindings';
import type { SavedKeyPassphrase, VaultInfo } from '../../ipc';

type Asking = { reason: string; answer: (open: boolean) => void };

class VaultControl {
  info = $state<VaultInfo | null>(null);
  loadError = $state('');
  asking = $state<Asking | null>(null);
  changing = $state(false);
  notice = $state('');
  keys = $state<SavedKeyPassphrase[]>([]);
  keysError = $state('');
  forgetting = $state<SavedKeyPassphrase | null>(null);
  forgetError = $state('');
  keysNotice = $state('');
  busy = $state(false);

  async refresh() {
    try {
      this.info = await commands.vaultInfo();
      this.loadError = '';
    } catch (e) {
      this.loadError = String(e);
    }
    return this.info;
  }

  // Only an open Vault can say which keys it holds a passphrase for.
  async loadKeys() {
    if (this.info?.state !== 'unlocked') {
      this.keys = [];
      return;
    }
    try {
      this.keys = await commands.vaultKeyPassphrases();
      this.keysError = '';
    } catch (e) {
      this.keysError = String(e);
    }
  }

  askForget(key: SavedKeyPassphrase) {
    this.forgetError = '';
    this.forgetting = key;
  }

  cancelForget() {
    this.forgetting = null;
  }

  async confirmForget() {
    const key = this.forgetting;
    if (!key || this.busy) return;
    this.busy = true;
    try {
      await commands.vaultForgetKeyPassphrase(key.credential);
      this.forgetting = null;
      this.keysNotice = `Forgot the passphrase of ${key.path}.`;
    } catch (e) {
      this.forgetError = String(e);
    }
    this.busy = false;
    await this.refresh();
    await this.loadKeys();
  }

  /** Creates or unlocks the Vault first if it has to. False when the user declined. */
  async ensureOpen(reason: string): Promise<boolean> {
    const info = await this.refresh();
    if (info?.state === 'unlocked') return true;
    return new Promise((answer) => (this.asking = { reason, answer }));
  }

  ask(reason: string) {
    void this.ensureOpen(reason);
  }

  answer(open: boolean) {
    const asking = this.asking;
    this.asking = null;
    asking?.answer(open);
  }

  async create(master: string) {
    await commands.vaultCreate(master);
    await this.refresh();
  }

  async unlock(master: string) {
    await commands.vaultUnlock(master);
    await this.refresh();
  }

  async changeMaster(current: string, replacement: string) {
    await commands.vaultChangeMaster(current, replacement);
    this.notice = 'Master password changed.';
    await this.refresh();
  }

  async setIdleMinutes(minutes: number) {
    await commands.vaultSetIdleMinutes(minutes);
    await this.refresh();
  }

  async lock() {
    await commands.vaultLock();
    await this.refresh();
  }
}

export const vault = new VaultControl();
