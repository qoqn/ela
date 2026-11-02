import { commands } from '../../bindings';
import type { KnownHost, KnownHostSection } from '../../ipc';
import { shownName } from './rows';

type Removing = { section: KnownHostSection; entry: KnownHost };

class KnownHosts {
  sections = $state<KnownHostSection[]>([]);
  loaded = $state(false);
  loadError = $state('');
  filter = $state('');
  removing = $state<Removing | null>(null);
  removeError = $state('');
  busy = $state(false);
  notice = $state('');
  closed = $state<Set<string>>(new Set());

  toggle(store: string) {
    const next = new Set(this.closed);
    if (!next.delete(store)) next.add(store);
    this.closed = next;
  }

  async load() {
    try {
      this.sections = await commands.knownHostsList();
      this.loadError = '';
    } catch (e) {
      this.loadError = String(e);
    }
    this.loaded = true;
  }

  ask(section: KnownHostSection, entry: KnownHost) {
    this.removeError = '';
    this.removing = { section, entry };
  }

  cancel() {
    this.removing = null;
  }

  async confirm() {
    if (!this.removing || this.busy) return;
    const { section, entry } = this.removing;
    this.busy = true;
    try {
      await commands.knownHostsRemove(section.store, entry.name, entry.fingerprint);
      this.removing = null;
      this.notice = `Removed ${shownName(entry)}. The previous file is kept as known_hosts.old.`;
    } catch (e) {
      this.removeError = String(e);
    }
    this.busy = false;
    await this.load();
  }
}

export const knownHosts = new KnownHosts();
