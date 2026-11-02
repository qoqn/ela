import { commands } from '../../bindings';
import type {
  CredentialRef,
  FieldChange,
  ItemRef,
  ResolvedConnection,
  SavedConnection,
  SavedFolder,
  SavedProblem,
  TrashEntry,
  TrashView,
} from '../../ipc';
import { workspace } from '../../sessions.svelte';
import { vault } from '../vault/vault.svelte';
import { rowKey, buildRows } from './tree';

const EXPANDED_SETTING = 'ela.tree.expanded';
const UNDO_MS = 8000;

/** A plain copy of reactive state. Option values are too deeply typed for `$state.snapshot`'s type. */
const plain = <T>(value: T): T => $state.snapshot(value as never) as T;

type Editing =
  | { kind: 'connection'; draft: SavedConnection; isNew: boolean }
  | { kind: 'folder'; draft: SavedFolder; isNew: boolean };

export type Moving = {
  item: ItemRef;
  name: string;
  dropped?: { to: string | null; changes: FieldChange[] };
};

export type ImportResult = { title: string; detail: string; notes: string[] };

type Toast = { message: string; detail?: string; undo?: () => void; error?: boolean };

function savedExpanded(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(EXPANDED_SETTING) ?? '[]'));
  } catch {
    return new Set();
  }
}

class Saved {
  connections = $state<SavedConnection[]>([]);
  folders = $state<SavedFolder[]>([]);
  problems = $state<SavedProblem[]>([]);
  loadError = $state('');
  expanded = $state(savedExpanded());
  active = $state<string | null>(null);
  filter = $state('');
  rows = $derived(
    buildRows({ connections: this.connections, folders: this.folders }, this.expanded, this.filter),
  );
  renaming = $state<string | null>(null);
  editing = $state<Editing | null>(null);
  moving = $state<Moving | null>(null);
  imported = $state<ImportResult | null>(null);
  askingImport = $state(false);
  toast = $state<Toast | null>(null);
  trashed = $state<TrashView | null>(null);
  trashError = $state('');
  trashOpen = $state(false);
  #toastTimer: ReturnType<typeof setTimeout> | undefined;
  onOpened: ((id: string) => void) | null = null;
  focusTree: () => void = () => {};
  focusSearch: () => void = () => {};

  get activeRow() {
    return this.rows.find((row) => row.key === this.active);
  }

  search(text: string) {
    this.filter = text;
    if (!text.trim()) return;
    const first = this.rows.find((row) => row.item.kind === 'connection') ?? this.rows[0];
    if (first) this.active = first.key;
  }

  async load() {
    try {
      const tree = await commands.listTree();
      this.connections = tree.connections;
      this.folders = tree.folders;
      this.problems = tree.problems;
      this.loadError = '';
    } catch (e) {
      this.loadError = String(e);
    }
    await this.#loadTrash();
  }

  async #loadTrash() {
    try {
      this.trashed = await commands.listTrash();
      this.trashError = '';
    } catch (e) {
      this.trashError = String(e);
    }
  }

  name(item: ItemRef) {
    const list: { id: string; name: string }[] =
      item.kind === 'connection' ? this.connections : this.folders;
    return list.find((each) => each.id === item.id)?.name ?? '';
  }

  showToast(toast: Toast) {
    clearTimeout(this.#toastTimer);
    this.toast = toast;
    this.#toastTimer = setTimeout(() => (this.toast = null), UNDO_MS);
  }

  dismissToast() {
    clearTimeout(this.#toastTimer);
    this.toast = null;
  }

  #fail(message: string, error: unknown) {
    this.showToast({ message, detail: String(error), error: true });
  }

  setExpanded(id: string, open: boolean) {
    const next = new Set(this.expanded);
    if (open) next.add(id);
    else next.delete(id);
    this.expanded = next;
    localStorage.setItem(EXPANDED_SETTING, JSON.stringify([...next]));
  }

  currentFolder(): string | undefined {
    const [kind, id] = this.active?.split(':') ?? [];
    if (kind === 'folder') return id;
    return this.connections.find((c) => c.id === id)?.folder;
  }

  newConnection(folder = this.currentFolder()) {
    const protocol = workspace.protocols.find((p) => p.listed)?.id ?? '';
    this.editing = {
      kind: 'connection',
      isNew: true,
      draft: { id: crypto.randomUUID(), name: '', folder, protocol, host: '' },
    };
  }

  newFolder(parent = this.currentFolder()) {
    this.editing = {
      kind: 'folder',
      isNew: true,
      draft: { id: crypto.randomUUID(), name: '', parent },
    };
  }

  edit(item: ItemRef) {
    if (item.kind === 'connection') {
      const draft = this.connections.find((c) => c.id === item.id);
      if (draft) this.editing = { kind: 'connection', isNew: false, draft: plain(draft) };
    } else {
      const draft = this.folders.find((f) => f.id === item.id);
      if (draft) this.editing = { kind: 'folder', isNew: false, draft: plain(draft) };
    }
  }

  /** Resolves to false, with nothing saved, when the user leaves the Vault closed. */
  async saveWithCredential(
    change: { label: string; current?: CredentialRef; password: string; removing: boolean },
    save: (credential: CredentialRef | undefined) => Promise<void>,
  ): Promise<boolean> {
    const { label, current, password, removing } = change;
    const reason = password
      ? `To save the password of ${label}.`
      : `To remove the saved password of ${label}.`;
    if ((password || removing) && !(await vault.ensureOpen(reason))) return false;
    let created: CredentialRef | null = null;
    try {
      let credential = removing ? undefined : current;
      if (password) {
        credential = await commands.vaultStoreSecret(password, credential ?? null);
        if (!current || removing) created = credential;
      }
      await save(credential);
      created = null;
      if (removing && current) await commands.vaultRemoveSecret(current);
      return true;
    } catch (e) {
      if (created) await commands.vaultRemoveSecret(created).catch(() => {});
      throw e;
    }
  }

  async saveConnection(connection: SavedConnection) {
    await commands.saveConnection(connection);
    await this.load();
    this.#reveal(connection.folder);
    this.active = rowKey({ kind: 'connection', id: connection.id });
  }

  async saveFolder(folder: SavedFolder) {
    await commands.saveFolder(folder);
    await this.load();
    this.#reveal(folder.parent);
    this.active = rowKey({ kind: 'folder', id: folder.id });
  }

  show(id: string) {
    const connection = this.connections.find((c) => c.id === id);
    if (!connection) return;
    this.filter = '';
    this.#reveal(connection.folder);
    this.active = rowKey({ kind: 'connection', id });
    this.focusTree();
  }

  #reveal(folder: string | null | undefined) {
    let id = folder;
    const seen = new Set<string>();
    while (id && !seen.has(id)) {
      seen.add(id);
      if (!this.expanded.has(id)) this.setExpanded(id, true);
      id = this.folders.find((f) => f.id === id)?.parent;
    }
  }

  async rename(item: ItemRef, name: string) {
    this.renaming = null;
    const trimmed = name.trim();
    if (!trimmed || trimmed === this.name(item)) return;
    try {
      if (item.kind === 'connection') {
        const connection = this.connections.find((c) => c.id === item.id);
        if (connection) await commands.saveConnection({ ...plain(connection), name: trimmed });
      } else {
        const folder = this.folders.find((f) => f.id === item.id);
        if (folder) await commands.saveFolder({ ...plain(folder), name: trimmed });
      }
      await this.load();
    } catch (e) {
      this.#fail(`Couldn't rename ${this.name(item)}`, e);
    }
  }

  async trash(item: ItemRef) {
    const name = this.name(item);
    try {
      const moved = await commands.trashItem(item);
      await this.load();
      const inside = moved.length - 1;
      const detail = inside
        ? `${name} and ${inside} ${inside === 1 ? 'item' : 'items'} inside`
        : name;
      this.showToast({
        message: 'Moved to Trash',
        detail,
        undo: () => void this.#restore(moved, name),
      });
    } catch (e) {
      this.#fail(`Couldn't move ${name} to Trash`, e);
    }
  }

  async #restore(items: ItemRef[], name: string) {
    this.dismissToast();
    try {
      await commands.restoreItems(items);
      await this.load();
    } catch (e) {
      this.#fail(`Couldn't bring back ${name}`, e);
    }
  }

  openTrash() {
    // An Undo left showing could name something this view has already restored or emptied.
    this.dismissToast();
    this.trashOpen = true;
    void this.#loadTrash();
  }

  async restoreFromTrash(entry: TrashEntry) {
    await commands.restoreItems([entry.item, ...entry.inside]);
    await this.load();
    const own =
      entry.item.kind === 'connection'
        ? this.connections.find((c) => c.id === entry.item.id)?.folder
        : this.folders.find((f) => f.id === entry.item.id)?.parent;
    this.#reveal(own);
    this.active = rowKey(entry.item);
  }

  /** Resolves to false, with nothing removed, when the user leaves the Vault closed. */
  async emptyTrash(): Promise<boolean> {
    if (this.trashed?.saved_credentials) {
      const info = await vault.refresh();
      if (!info) throw vault.loadError;
      const reason = 'To remove the saved passwords of the items in the Trash.';
      if (info.state !== 'absent' && !(await vault.ensureOpen(reason))) return false;
    }
    await commands.emptyTrash();
    await this.load();
    const left = this.trashed?.problems.length ?? 0;
    this.showToast({
      message: 'Trash emptied',
      detail: left
        ? `${left} ${left === 1 ? "file can't be read and stays" : "files can't be read and stay"} there.`
        : undefined,
    });
    return true;
  }

  async drop(item: ItemRef, to: string | null) {
    try {
      const changes = await commands.moveChanges(item, to);
      if (changes.length) this.moving = { item, name: this.name(item), dropped: { to, changes } };
      else await this.move(item, to);
    } catch (e) {
      this.#fail(`Couldn't move ${this.name(item)}`, e);
    }
  }

  async move(item: ItemRef, to: string | null) {
    this.moving = null;
    try {
      await commands.moveItem(item, to);
      await this.load();
      this.#reveal(to);
      this.active = rowKey(item);
    } catch (e) {
      this.#fail(`Couldn't move ${this.name(item)}`, e);
    }
  }

  /** The Tab asks for whatever the Session needs, the Vault included. */
  async connect(id: string, replacingTab?: string) {
    try {
      const target = await commands.resolveConnection(id);
      workspace.open(
        {
          protocol: target.protocol,
          host: target.host,
          port: target.port,
          username: target.username,
          options: target.options as never,
        },
        { name: target.name, env: target.environment, connection: target.id },
        replacingTab,
      );
      this.onOpened?.(target.id);
    } catch (e) {
      this.#fail("Couldn't connect", e);
    }
  }

  async importSshConfig() {
    try {
      const { added, already_saved, notes } = await commands.importSshConfig();
      const title = added
        ? `Imported ${added} ${added === 1 ? 'connection' : 'connections'}`
        : 'Nothing new in ~/.ssh/config';
      const detail = already_saved ? `${already_saved} already saved.` : '';
      if (notes.length) this.imported = { title, detail, notes };
      else this.showToast({ message: title, detail: detail || undefined });
    } catch (e) {
      this.#fail("Couldn't import ~/.ssh/config", e);
    }
    await this.load();
  }

  editById(id: string) {
    if (this.connections.some((c) => c.id === id)) this.edit({ kind: 'connection', id });
  }
}

export const saved = new Saved();
workspace.onSavedChanged = () => void saved.load();
