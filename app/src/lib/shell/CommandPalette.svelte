<script lang="ts">
  import { tick } from 'svelte';
  import { openedAgo } from '../features/tree/recent';
  import { recent } from '../features/tree/recent.svelte';
  import { saved } from '../features/tree/saved.svelte';
  import { buildRows } from '../features/tree/tree';
  import { workspace } from '../sessions.svelte';
  import { EnvRail, Kbd, type Env } from '../ui';
  import { fuzzyScore, looksLikeAddress } from './palette';
  import { runCommand, shell } from './shell.svelte';
  import { SHORTCUTS, type AppCommand } from './shortcuts';

  type Result = {
    id: string;
    group: string;
    label: string;
    detail?: string;
    keys?: string;
    note?: string;
    env?: Env;
    connection?: string;
    run: () => void;
  };

  const connecting = $derived(shell.palette === 'connect');
  // Set when the palette opens: which shortcuts apply depends on where focus came from.
  const previous = document.activeElement as HTMLElement | null;
  const column =
    previous?.tagName === 'CANVAS'
      ? 'desktop'
      : previous?.closest('.xterm')
        ? 'terminal'
        : 'elsewhere';

  let query = $state('');
  let selected = $state(0);
  let input = $state<HTMLInputElement>();

  const needsTab: AppCommand[] = ['close_tab', 'next_tab', 'previous_tab'];

  const results = $derived.by((): Result[] => {
    const address = query.trim();
    const connect: Result[] = looksLikeAddress(address)
      ? [
          {
            id: 'connect',
            group: 'Connect',
            label: `Connect to ${address}`,
            run: () => shell.connectTo(address),
          },
        ]
      : [];
    const tabs = workspace.tabs.map((tab) => ({
      id: `tab-${tab.key}`,
      group: 'Open tabs',
      label: tab.name,
      detail: tab.address,
      env: tab.env,
      run: () => workspace.select(tab.key),
    }));
    const connections = buildRows(
      { connections: saved.connections, folders: saved.folders },
      new Set(saved.folders.map((f) => f.id)),
    )
      .filter((row) => row.item.kind === 'connection')
      .map((row) => ({
        id: `connection-${row.item.id}`,
        connection: row.item.id,
        group: 'Connections',
        label: row.name,
        detail: row.detail,
        env: row.env,
        run: () => void saved.connect(row.item.id),
      }));
    const importing: Result[] = connecting
      ? []
      : [
          {
            id: 'import-ssh-config',
            group: 'Commands',
            label: 'Import SSH hosts',
            detail: '~/.ssh/config',
            run: () => (saved.askingImport = true),
          },
          {
            id: 'open-trash',
            group: 'Commands',
            label: 'Open Trash',
            run: () => saved.openTrash(),
          },
        ];
    const commands = connecting
      ? []
      : SHORTCUTS.flatMap((row) => {
          const { command } = row;
          if (!command || (needsTab.includes(command) && !workspace.tabs.length)) return [];
          return [
            {
              id: command,
              group: 'Commands',
              label: row.action,
              keys: row[column] ?? undefined,
              run: () => runCommand(command),
            },
          ];
        });

    const matching = (items: Result[]) =>
      items
        .map((item) => ({ item, score: fuzzyScore(query, `${item.label} ${item.detail ?? ''}`) }))
        .filter((match) => match.score !== null)
        .sort((a, b) => b.score! - a.score!)
        .map((match) => match.item);
    // Before anything is typed, the machines last used come first, newest first.
    const recentFirst: Result[] = query.trim()
      ? []
      : recent.rows.map((row) => ({
          id: `recent-${row.id}`,
          group: 'Recent',
          label: row.name,
          detail: row.detail,
          note: openedAgo(row.opened, recent.now),
          env: row.env,
          connection: row.id,
          run: () => void saved.connect(row.id),
        }));
    const listed = new Set(recentFirst.map((item) => item.connection));
    return [
      ...connect,
      ...matching(tabs),
      ...recentFirst,
      ...matching(connections.filter((item) => !listed.has(item.connection))),
      ...matching([...commands, ...importing]),
    ];
  });

  $effect(() => {
    input?.focus();
    return () => previous?.focus();
  });

  function close() {
    shell.palette = null;
  }

  async function run(result: Result | undefined) {
    if (!result) return;
    close();
    // After focus has returned to where it was, so a command can move it elsewhere.
    await tick();
    requestAnimationFrame(result.run);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'Enter') void run(results[selected]);
    else if (e.key === 'ArrowDown') selected = (selected + 1) % Math.max(results.length, 1);
    else if (e.key === 'ArrowUp')
      selected = (selected - 1 + results.length) % Math.max(results.length, 1);
    else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 z-40 flex justify-center bg-screen/60 p-4 pt-[12vh]"
  onclick={(e) => e.target === e.currentTarget && close()}
>
  <div
    role="dialog"
    aria-modal="true"
    aria-label={connecting ? 'New tab' : 'Command palette'}
    class="flex max-h-[60vh] w-full max-w-[64ch] flex-col self-start overflow-hidden rounded-md border border-line bg-panel shadow-2"
  >
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => (selected = 0)}
      onkeydown={onKeydown}
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-results"
      aria-activedescendant={results[selected] ? `palette-${results[selected].id}` : undefined}
      aria-label={connecting ? 'Address' : 'Search connections, commands and tabs'}
      spellcheck="false"
      autocomplete="off"
      placeholder={connecting
        ? 'Search connections, or type an address'
        : 'Search connections, commands and tabs, or type an address'}
      class="h-9 shrink-0 border-b border-line bg-transparent px-3 text-text outline-none placeholder:text-dim"
    />
    <ul id="palette-results" role="listbox" class="min-h-0 overflow-y-auto py-1">
      {#each results as result, index (result.id)}
        {#if index === 0 || results[index - 1].group !== result.group}
          <li role="presentation" class="px-3 pt-2 pb-1 text-xs text-dim">{result.group}</li>
        {/if}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id="palette-{result.id}"
          role="option"
          aria-selected={index === selected}
          class="flex h-7 cursor-default items-center gap-2 px-3 text-sm"
          class:bg-selected={index === selected}
          onmousemove={() => (selected = index)}
          onclick={() => run(result)}
        >
          {#if result.env}<span class="h-4"
              ><EnvRail env={result.env} orientation="vertical" /></span
            >{/if}
          <span class="truncate">{result.label}</span>
          {#if result.detail}<span class="truncate font-mono text-xs text-dim">{result.detail}</span
            >{/if}
          {#if result.note}<span class="ml-auto shrink-0 pl-2 text-xs text-dim tabular"
              >{result.note}</span
            >{/if}
          {#if result.keys}<span class="ml-auto shrink-0"><Kbd keys={result.keys} /></span>{/if}
        </li>
      {:else}
        <li role="presentation" class="px-3 py-2 text-sm text-dim">
          {query.trim()
            ? `Nothing matches “${query.trim()}”.`
            : 'No connections yet. Type an address to connect.'}
        </li>
      {/each}
    </ul>
  </div>
</div>
