import { Channel } from '@tauri-apps/api/core';
import { commands } from '../../bindings';
import type { RemoteEntry, RemoteListing, TransferEvent } from '../../ipc';
import type { SessionTab } from '../../sessions.svelte';

export type TransferRow = {
  key: string;
  transfer: string;
  direction: 'up' | 'down';
  name: string;
  done: number;
  total: number;
  state: 'running' | 'finished' | 'cancelled' | 'failed';
  message?: string;
  note?: string;
};

export type Naming = { kind: 'new' } | { kind: 'rename'; entry: RemoteEntry };
export type FilesMenu = { entry: RemoteEntry; x: number; y: number };

export class FileBrowser {
  readonly #tab: SessionTab;
  listing = $state<RemoteListing | null>(null);
  error = $state('');
  loading = $state(false);
  showHidden = $state(false);
  selected = $state<string | null>(null);
  transfers = $state<TransferRow[]>([]);
  menu = $state<FilesMenu | null>(null);
  naming = $state<Naming | null>(null);
  nameText = $state('');
  nameError = $state('');
  deleting = $state<RemoteEntry | null>(null);
  deleteError = $state('');
  busy = $state(false);
  dragOver = $state(false);

  constructor(tab: SessionTab) {
    this.#tab = tab;
  }

  get entries(): RemoteEntry[] {
    const entries = this.listing?.entries ?? [];
    return this.showHidden ? entries : entries.filter((entry) => !entry.name.startsWith('.'));
  }

  async open(path: string | null) {
    if (!this.#tab.id) return;
    this.loading = true;
    this.error = '';
    try {
      this.listing = await commands.filesList(this.#tab.id, path);
      this.selected = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  refresh() {
    return this.open(this.listing?.path ?? null);
  }

  #channel(transfer: string, direction: TransferRow['direction'], after?: () => void) {
    const channel = new Channel<TransferEvent>((event) => {
      let row = this.transfers.find((t) => t.transfer === transfer && t.name === event.name);
      if (!row) {
        this.transfers.push({
          key: crypto.randomUUID(),
          transfer,
          direction,
          name: event.name,
          done: 0,
          total: 0,
          state: 'running',
        });
        row = this.transfers[this.transfers.length - 1];
      }
      if (event.type === 'progress') {
        row.done = event.done;
        row.total = event.total;
      } else if (event.type === 'failed') {
        row.state = 'failed';
        row.message = event.message;
      } else {
        row.state = event.type;
        if (event.type === 'finished' && event.note) row.note = event.note;
      }
      if (event.type !== 'progress') after?.();
    });
    return channel as never;
  }

  async download(entry: RemoteEntry) {
    if (!this.#tab.id) return;
    const transfer = crypto.randomUUID();
    const start = entry.kind === 'folder' ? commands.filesDownloadFolder : commands.filesDownload;
    try {
      await start(this.#tab.id, entry.path, transfer, this.#channel(transfer, 'down'));
    } catch (e) {
      this.error = String(e);
    }
  }

  async #upload(
    start: (id: string, directory: string, transfer: string, events: never) => Promise<null>,
  ) {
    if (!this.#tab.id || !this.listing) return;
    const transfer = crypto.randomUUID();
    const refresh = () => void this.refresh();
    try {
      await start(
        this.#tab.id,
        this.listing.path,
        transfer,
        this.#channel(transfer, 'up', refresh),
      );
    } catch (e) {
      this.error = String(e);
    }
  }

  upload() {
    return this.#upload(commands.filesUpload);
  }

  uploadFolder() {
    return this.#upload(commands.filesUploadFolder);
  }

  uploadDropped(token: string) {
    return this.#upload((id, directory, transfer, events) =>
      commands.filesUploadDropped(id, directory, token, transfer, events),
    );
  }

  startNewFolder() {
    this.naming = { kind: 'new' };
    this.nameText = '';
    this.nameError = '';
  }

  startRename(entry: RemoteEntry) {
    this.naming = { kind: 'rename', entry };
    this.nameText = entry.name;
    this.nameError = '';
  }

  async saveName() {
    if (!this.#tab.id || !this.listing || !this.naming || this.busy) return;
    const name = this.nameText.trim();
    const parent = this.listing.path;
    this.busy = true;
    this.nameError = '';
    try {
      if (this.naming.kind === 'new') {
        await commands.filesMakeFolder(this.#tab.id, parent, name);
      } else {
        await commands.filesRename(this.#tab.id, this.naming.entry.path, name);
      }
      this.naming = null;
      await this.refresh();
      this.selected = this.listing?.entries.find((entry) => entry.name === name)?.path ?? null;
    } catch (e) {
      this.nameError = String(e);
    } finally {
      this.busy = false;
    }
  }

  startDelete(entry: RemoteEntry) {
    this.deleting = entry;
    this.deleteError = '';
  }

  async confirmDelete() {
    if (!this.#tab.id || !this.deleting || this.busy) return;
    this.busy = true;
    this.deleteError = '';
    try {
      await commands.filesDelete(this.#tab.id, this.deleting.path);
      this.deleting = null;
    } catch (e) {
      this.deleteError = String(e);
    } finally {
      this.busy = false;
      // A folder that stopped partway has already lost some of its contents.
      await this.refresh();
    }
  }

  cancel(row: TransferRow) {
    void commands.filesCancel(row.transfer);
  }

  dismiss(row: TransferRow) {
    this.transfers = this.transfers.filter((t) => t.key !== row.key);
  }
}

const browsers = new WeakMap<SessionTab, FileBrowser>();

export function fileBrowserOf(tab: SessionTab): FileBrowser {
  let browser = browsers.get(tab);
  if (!browser) {
    browser = new FileBrowser(tab);
    browsers.set(tab, browser);
  }
  return browser;
}
