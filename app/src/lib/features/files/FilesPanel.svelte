<script lang="ts">
  import {
    ArrowUp,
    Download,
    Eye,
    EyeOff,
    File,
    FileQuestion,
    Folder,
    FolderPlus,
    FolderUp,
    Link,
    RefreshCw,
    Upload,
    X,
  } from '@lucide/svelte';
  import { listen } from '@tauri-apps/api/event';
  import { VList } from 'virtua/svelte';
  import type { FileDrag, RemoteEntry } from '../../ipc';
  import { workspace } from '../../sessions.svelte';
  import { Button, Dialog, EmptyState, IconButton, Menu, Panel, TextField } from '../../ui';
  import { reconnect } from '../sessions/reconnect.svelte';
  import type { MenuItem } from '../../ui';
  import { fileBrowserOf, type TransferRow } from './files.svelte';
  import { deleteWording, formatModified, formatSize, percent } from './format';

  const ROW_HEIGHT = 26;

  const tab = $derived(workspace.active);
  const offersFiles = $derived(
    !!tab && !!workspace.protocol(tab.protocol)?.capabilities.file_transfer,
  );
  const browser = $derived(tab && offersFiles ? fileBrowserOf(tab) : null);
  const connected = $derived(tab?.status.state === 'connected');

  let typedPath = $state('');
  let list = $state<HTMLElement>();
  let panel = $state<HTMLElement>();

  $effect(() => {
    if (browser && connected && !browser.listing && !browser.error && !browser.loading) {
      void browser.open(null);
    }
  });

  $effect(() => {
    typedPath = browser?.listing?.path ?? '';
  });

  function overPanel(x: number, y: number) {
    const box = panel?.getBoundingClientRect();
    return !!box && x >= box.left && x < box.right && y >= box.top && y < box.bottom;
  }

  $effect(() => {
    const stop = listen<FileDrag>('file-drag', ({ payload }) => {
      const target = connected && browser?.listing ? browser : null;
      if (browser) browser.dragOver = false;
      if (!target || payload.type === 'left') return;
      const over = overPanel(payload.x, payload.y);
      if (payload.type === 'over') target.dragOver = over;
      else if (over) void target.uploadDropped(payload.token);
    });
    return () => void stop.then((unlisten) => unlisten());
  });

  const selectedEntry = $derived(browser?.entries.find((e) => e.path === browser?.selected));

  function activate(entry: RemoteEntry) {
    if (entry.kind === 'folder') void browser?.open(entry.path).then(() => list?.focus());
    else if (entry.kind === 'file') void browser?.download(entry);
  }

  function menuItems(entry: RemoteEntry): MenuItem[] {
    const items: MenuItem[] = [];
    if (entry.kind === 'folder') {
      items.push({ label: 'Open', keys: 'Enter', run: () => activate(entry) });
    }
    if (entry.kind !== 'other') {
      items.push({ label: 'Download', run: () => void browser?.download(entry) });
    }
    items.push(
      { label: 'Rename', keys: 'F2', run: () => browser?.startRename(entry) },
      { label: 'Delete', keys: 'Del', run: () => browser?.startDelete(entry) },
    );
    return items;
  }

  function onListKey(e: KeyboardEvent) {
    if (!browser) return;
    const entries = browser.entries;
    const at = entries.findIndex((entry) => entry.path === browser?.selected);
    const moves: Record<string, number> = {
      ArrowDown: Math.min(at + 1, entries.length - 1),
      ArrowUp: Math.max(at - 1, 0),
      Home: 0,
      End: entries.length - 1,
    };
    if (e.key in moves && entries.length) {
      e.preventDefault();
      browser.selected = entries[moves[e.key]].path;
    } else if (e.key === 'Enter' && selectedEntry) {
      e.preventDefault();
      activate(selectedEntry);
    } else if (e.key === 'Backspace' && browser.listing?.parent) {
      e.preventDefault();
      void browser.open(browser.listing.parent);
    } else if (e.key === 'F2' && selectedEntry) {
      e.preventDefault();
      browser.startRename(selectedEntry);
    } else if (e.key === 'Delete' && selectedEntry) {
      e.preventDefault();
      browser.startDelete(selectedEntry);
    } else if ((e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) && selectedEntry) {
      e.preventDefault();
      const box = document.getElementById(`file-${selectedEntry.path}`)?.getBoundingClientRect();
      browser.menu = { entry: selectedEntry, x: (box?.left ?? 0) + 24, y: box?.bottom ?? 0 };
    }
  }

  function saveName(e: SubmitEvent) {
    e.preventDefault();
    void browser?.saveName();
  }

  function goToTyped(e: SubmitEvent) {
    e.preventDefault();
    if (typedPath.trim()) void browser?.open(typedPath.trim());
  }

  const stateText: Record<TransferRow['state'], string> = {
    running: '',
    finished: 'Done',
    cancelled: 'Cancelled',
    failed: 'Failed',
  };
</script>

<Panel bind:ref={panel} title={tab && browser ? `Files on ${tab.name}` : 'Files'}>
  {#if !browser}
    <EmptyState title="Files work over SSH." detail="Open an SSH session to browse its files." />
  {:else if tab?.status.state === 'closed'}
    <EmptyState title="This session has ended." detail="Reconnect to browse its files.">
      {#snippet actions()}
        {#if tab}<Button size="sm" variant="primary" onclick={() => reconnect.start(tab)}
            >Reconnect</Button
          >{/if}
      {/snippet}
    </EmptyState>
  {:else if !connected}
    <EmptyState title="Waiting for {tab?.name}…" live />
  {:else}
    <div class="flex items-center gap-1 px-1.5 pb-2">
      <IconButton
        label="Up one folder"
        disabled={!browser.listing?.parent}
        onclick={() => browser?.listing?.parent && void browser.open(browser.listing.parent)}
      >
        <ArrowUp size={15} />
      </IconButton>
      <form class="min-w-0 flex-1" onsubmit={goToTyped}>
        <TextField mono bind:value={typedPath} aria-label="Remote folder" />
      </form>
      <IconButton label="New folder" onclick={() => browser?.startNewFolder()}>
        <FolderPlus size={15} />
      </IconButton>
      <IconButton label="Refresh" onclick={() => void browser?.refresh()}>
        <RefreshCw size={15} />
      </IconButton>
      <IconButton
        label={browser.showHidden ? 'Hide hidden files' : 'Show hidden files'}
        active={browser.showHidden}
        onclick={() => browser && (browser.showHidden = !browser.showHidden)}
      >
        {#if browser.showHidden}<Eye size={15} />{:else}<EyeOff size={15} />{/if}
      </IconButton>
    </div>

    {#if browser.error}
      <p class="mx-3 mb-1 text-sm text-danger-text" role="alert">{browser.error}</p>
    {/if}

    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={list}
      class="list-focus relative min-h-0 flex-1"
      role="listbox"
      aria-label="Remote files"
      tabindex="0"
      aria-activedescendant={browser.selected ? `file-${browser.selected}` : undefined}
      onkeydown={onListKey}
    >
      {#if browser.listing && !browser.entries.length}
        <EmptyState title="This folder is empty." />
      {:else}
        <VList data={browser.entries} getKey={(entry) => entry.path} itemSize={ROW_HEIGHT}>
          {#snippet children(entry)}
            {@const Icon =
              entry.kind === 'folder' ? Folder : entry.kind === 'file' ? File : FileQuestion}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              id="file-{entry.path}"
              role="option"
              tabindex="-1"
              aria-selected={browser?.selected === entry.path}
              data-current={browser?.selected === entry.path || undefined}
              class="flex h-6.5 items-center gap-2 px-2 text-sm {browser?.selected === entry.path
                ? 'bg-selected'
                : 'hover:bg-raised'} {entry.name.startsWith('.') ? 'text-dim' : 'text-text'}"
              onclick={() => browser && (browser.selected = entry.path)}
              ondblclick={() => activate(entry)}
              oncontextmenu={(e) => {
                e.preventDefault();
                if (!browser) return;
                browser.selected = entry.path;
                browser.menu = { entry, x: e.clientX, y: e.clientY };
              }}
            >
              <Icon size={14} class="shrink-0 text-dim" />
              <span class="min-w-0 flex-1 truncate" title={entry.name}>{entry.name}</span>
              {#if entry.link}<Link size={11} class="shrink-0 text-dim" />{/if}
              {#if entry.kind === 'file'}
                <span class="w-14 shrink-0 text-right text-xs text-dim">
                  {formatSize(entry.size)}
                </span>
              {/if}
              <span class="w-18 shrink-0 text-right text-xs whitespace-nowrap text-dim">
                {formatModified(entry.modified ?? null)}
              </span>
            </div>
          {/snippet}
        </VList>
      {/if}
      {#if browser.dragOver}
        <div
          class="pointer-events-none absolute inset-1 grid place-items-center rounded-sm border border-dashed border-signal bg-screen/80 p-4 text-center text-sm"
          role="status"
        >
          Drop to upload to {browser.listing?.path}
        </div>
      {/if}
    </div>

    <div class="flex gap-2 border-t border-line p-2">
      <Button size="sm" class="flex-1" onclick={() => void browser?.upload()}>
        <Upload size={13} /> Upload
      </Button>
      <IconButton label="Upload a folder" onclick={() => void browser?.uploadFolder()}>
        <FolderUp size={15} />
      </IconButton>
      <Button
        size="sm"
        class="flex-1"
        disabled={!selectedEntry || selectedEntry.kind === 'other'}
        onclick={() => selectedEntry && void browser?.download(selectedEntry)}
      >
        <Download size={13} /> Download
      </Button>
    </div>
  {/if}

  {#if browser?.transfers.length}
    <ul class="max-h-48 overflow-y-auto border-t border-line py-1" aria-label="Transfers">
      {#each browser.transfers as row (row.key)}
        <li class="px-2 py-1 text-sm">
          <div class="flex items-center gap-2">
            {#if row.direction === 'up'}<Upload size={12} class="shrink-0 text-dim" />
            {:else}<Download size={12} class="shrink-0 text-dim" />{/if}
            <span class="min-w-0 flex-1 truncate" title={row.name}>{row.name}</span>
            <span class="shrink-0 text-xs text-dim">
              {row.state === 'running' ? `${percent(row.done, row.total)}%` : stateText[row.state]}
            </span>
            {#if row.state === 'running'}
              <IconButton label="Cancel {row.name}" onclick={() => browser?.cancel(row)}>
                <X size={13} />
              </IconButton>
            {:else}
              <IconButton label="Dismiss {row.name}" onclick={() => browser?.dismiss(row)}>
                <X size={13} />
              </IconButton>
            {/if}
          </div>
          {#if row.state === 'running'}
            <div
              class="mt-1 h-0.5 w-full bg-chassis"
              role="progressbar"
              aria-label="{row.name} progress"
              aria-valuenow={percent(row.done, row.total)}
              aria-valuemin="0"
              aria-valuemax="100"
            >
              <div class="h-full bg-signal" style:width="{percent(row.done, row.total)}%"></div>
            </div>
          {/if}
          {#if row.message}
            <p class="mt-0.5 text-xs text-danger-text selectable">{row.message}</p>
          {/if}
          {#if row.note}
            <p class="mt-0.5 text-xs text-dim selectable">{row.note}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Panel>

{#if browser?.menu}
  <Menu
    items={menuItems(browser.menu.entry)}
    x={browser.menu.x}
    y={browser.menu.y}
    onclose={() => browser && (browser.menu = null)}
  />
{/if}

{#if browser?.naming}
  {@const renaming = browser.naming.kind === 'rename'}
  <Dialog
    title={browser.naming.kind === 'rename' ? `Rename ${browser.naming.entry.name}` : 'New folder'}
    onclose={() => browser && (browser.naming = null)}
  >
    <form id="files-name" onsubmit={saveName}>
      <TextField
        data-autofocus
        mono
        bind:value={browser.nameText}
        error={browser.nameError}
        aria-label="Name"
      />
    </form>
    {#snippet footer()}
      <Button onclick={() => browser && (browser.naming = null)}>Cancel</Button>
      <Button
        type="submit"
        form="files-name"
        variant="primary"
        disabled={browser?.busy || !browser?.nameText.trim()}
      >
        {renaming ? 'Rename' : 'Create folder'}
      </Button>
    {/snippet}
  </Dialog>
{/if}

{#if browser?.deleting && tab}
  {@const wording = deleteWording(browser.deleting, tab.name)}
  <Dialog title={wording.title} onclose={() => browser && (browser.deleting = null)}>
    <div class="space-y-3 text-sm">
      <p class="font-mono text-xs break-all selectable">{browser.deleting.path}</p>
      <p>{wording.body}</p>
      {#if browser.deleteError}
        <p class="text-danger-text selectable" role="alert">{browser.deleteError}</p>
      {/if}
    </div>
    {#snippet footer()}
      <Button data-autofocus onclick={() => browser && (browser.deleting = null)}>Cancel</Button>
      <Button
        variant="danger"
        disabled={browser?.busy}
        onclick={() => void browser?.confirmDelete()}
      >
        Delete
      </Button>
    {/snippet}
  </Dialog>
{/if}
