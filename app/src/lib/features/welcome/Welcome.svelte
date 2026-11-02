<script lang="ts">
  import { EnvRail, Kbd } from '../../ui';
  import logo from '../../ui/logo.svg';
  import { openedAgo } from '../tree/recent';
  import { recent } from '../tree/recent.svelte';
  import { runCommand } from '../../shell/shell.svelte';
  import { SHORTCUTS, type AppCommand } from '../../shell/shortcuts';
  import { saved } from '../tree/saved.svelte';

  let { loadError = '' }: { loadError?: string } = $props();

  const firstRun = $derived(!saved.connections.length && !saved.folders.length);
  // With nothing saved, the connect palette would open empty.
  // "New tab" says nothing on a screen without Tabs; what it does here is open a Connection.
  const commands = $derived<{ label: string; action: string; command: AppCommand }[]>([
    ...(firstRun
      ? []
      : [{ label: 'Open a connection', action: 'New tab', command: 'new_tab' as const }]),
    { label: 'Search or connect', action: 'Search or connect', command: 'search_or_connect' },
    { label: 'Command palette', action: 'Command palette', command: 'command_palette' },
    { label: 'Shortcut sheet', action: 'Shortcut sheet', command: 'shortcut_sheet' },
  ]);
  const row =
    '-mx-2 flex w-[calc(100%+1rem)] items-center justify-between gap-4 rounded-sm px-2 py-1.5 text-left text-text transition-colors duration-(--motion-fast) hover:bg-raised focus-visible:outline-1 focus-visible:-outline-offset-1 focus-visible:outline-signal';
  // Fewer than the sidebar keeps: this is a start page, not the history.
  const recentRows = $derived(recent.rows.slice(0, 5));
  const keys = (action: string) => SHORTCUTS.find((row) => row.action === action)?.elsewhere ?? '';
</script>

<div class="grid h-full place-items-center bg-chassis p-6">
  <div class="w-full max-w-[52ch]">
    <!-- Three screen pixels per logo pixel keeps its edges sharp. -->
    <img src={logo} alt="" width="57" height="48" class="mb-4" />
    <h1 class="text-xl font-semibold text-label">Connect to a machine</h1>
    <p class="mt-2 text-dim">
      {firstRun
        ? 'Type an address like root@10.0.0.5 in the search field, or import the hosts in your SSH config.'
        : 'Open a saved connection, or type an address like root@10.0.0.5 in the search field.'}
    </p>
    <ul class="mt-6 grid gap-0.5">
      {#if firstRun}
        <li>
          <button type="button" class={row} onclick={() => (saved.askingImport = true)}>
            <span>Import SSH hosts</span>
          </button>
        </li>
      {/if}
      {#each commands as { label, action, command } (command)}
        <li>
          <button type="button" class={row} onclick={() => runCommand(command)}>
            <span>{label}</span>
            <Kbd keys={keys(action)} />
          </button>
        </li>
      {/each}
    </ul>
    {#if recentRows.length}
      <h2 class="mt-8 text-sm font-semibold text-dim">Recent</h2>
      <ul class="mt-2 grid gap-0.5" aria-label="Recent connections">
        {#each recentRows as entry (entry.id)}
          <li>
            <button
              type="button"
              class={row}
              title={entry.path}
              onclick={() => void saved.connect(entry.id)}
            >
              <span class="flex min-w-0 items-center gap-2">
                <span class="h-4"><EnvRail env={entry.env} orientation="vertical" /></span>
                <span class="truncate">{entry.name}</span>
                {#if entry.detail}
                  <span class="truncate font-mono text-xs text-dim">{entry.detail}</span>
                {/if}
              </span>
              <span class="shrink-0 text-xs text-dim tabular">
                {openedAgo(entry.opened, recent.now)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if loadError}
      <p class="mt-6 text-sm text-danger-text" role="alert">
        Ela couldn't reach its backend: {loadError}
      </p>
    {/if}
  </div>
</div>
