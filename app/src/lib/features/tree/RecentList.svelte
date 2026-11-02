<script lang="ts">
  import { ChevronDown, ChevronRight } from '@lucide/svelte';
  import { tick } from 'svelte';
  import { workspace } from '../../sessions.svelte';
  import { ConnectionIcon, ENV_NAME, EnvRail, LinkBadge, Menu, type MenuItem } from '../../ui';
  import { linkText } from '../sessions/link';
  import { openedAgo, openedAt, type RecentRow } from './recent';
  import { recent } from './recent.svelte';
  import { saved } from './saved.svelte';

  const rows = $derived(recent.rows);
  // One tab stop for the whole list; the arrow keys move within it.
  let current = $state(0);
  let list = $state<HTMLElement>();
  let menu = $state<{ row: RecentRow; x: number; y: number } | null>(null);

  recent.load();

  $effect(() => {
    if (current >= rows.length) current = Math.max(rows.length - 1, 0);
  });

  function describe(row: RecentRow) {
    const link = workspace.linkOf(row.id);
    return [
      row.name,
      ENV_NAME[row.env],
      ...(link ? [linkText(link)] : []),
      `opened ${openedAgo(row.opened, recent.now).toLowerCase()}`,
    ].join(', ');
  }

  function tooltip(row: RecentRow) {
    const where = row.detail ? `${row.path}  ·  ${row.detail}` : row.path;
    return `${where}\nOpened ${openedAt(row.opened)}`;
  }

  async function focusRow(index: number) {
    current = index;
    await tick();
    list?.querySelectorAll<HTMLElement>('button[data-row]')[index]?.focus();
  }

  async function forget(row: RecentRow) {
    const index = rows.indexOf(row);
    const hadFocus = list?.contains(document.activeElement);
    await recent.forget(row.id);
    if (hadFocus && rows.length) void focusRow(Math.min(index, rows.length - 1));
  }

  function menuItems(row: RecentRow): MenuItem[] {
    return [
      { label: 'Connect', keys: 'Enter', run: () => void saved.connect(row.id) },
      { label: 'Show in tree', run: () => saved.show(row.id) },
      {
        label: 'Edit',
        keys: 'Alt+Enter',
        run: () => saved.edit({ kind: 'connection', id: row.id }),
      },
      { label: 'Remove from Recent', keys: 'Delete', run: () => void forget(row) },
      { label: 'Clear Recent', run: () => void recent.clear() },
    ];
  }

  function onKeydown(e: KeyboardEvent, row: RecentRow, index: number) {
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey;
    if (e.key === 'ArrowDown' && plain) void focusRow(Math.min(index + 1, rows.length - 1));
    else if (e.key === 'ArrowUp' && plain) void focusRow(Math.max(index - 1, 0));
    else if (e.key === 'Home' && plain) void focusRow(0);
    else if (e.key === 'End' && plain) void focusRow(rows.length - 1);
    else if (e.key === 'Delete' && plain) void forget(row);
    else if (e.key === 'Enter' && e.altKey) saved.edit({ kind: 'connection', id: row.id });
    else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
      menu = { row, x: box.left + 24, y: box.bottom };
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

{#if saved.connections.length}
  <section class="shrink-0 border-t border-line" aria-labelledby="recent-heading">
    <h3 id="recent-heading">
      <button
        data-row
        class="flex h-7 w-full items-center gap-1 pr-3 pl-1 text-sm font-semibold text-dim hover:text-text"
        aria-expanded={!recent.folded}
        onclick={() => recent.setFolded(!recent.folded)}
      >
        <span class="grid size-4 place-items-center">
          {#if recent.folded}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
        </span>
        <span class="flex-1 text-left">Recent</span>
        {#if recent.folded && rows.length}
          <span class="text-xs font-normal tabular">{rows.length}</span>
        {/if}
      </button>
    </h3>
    {#if !recent.folded}
      {#if recent.loadError}
        <p class="px-3 pb-2 text-sm text-danger-text selectable" role="alert">
          Recent can't be read: {recent.loadError}
        </p>
      {:else if !rows.length}
        <p class="px-3 pb-2 text-sm text-dim">Connections you open show up here.</p>
      {:else}
        <ul bind:this={list} class="pb-1" aria-label="Recent connections">
          {#each rows as row, index (row.id)}
            {@const link = workspace.linkOf(row.id)}
            <li>
              <button
                data-row
                tabindex={index === current ? 0 : -1}
                class="flex h-6.5 w-full items-stretch text-left hover:bg-raised focus-visible:bg-raised"
                aria-label={describe(row)}
                title={tooltip(row)}
                onfocus={() => (current = index)}
                onclick={() => void saved.connect(row.id)}
                onkeydown={(e) => onKeydown(e, row, index)}
                oncontextmenu={(e) => {
                  e.preventDefault();
                  current = index;
                  menu = { row, x: e.clientX, y: e.clientY };
                }}
              >
                <span class="flex min-w-0 flex-1 items-center gap-1 pr-2 pl-6">
                  <span class="shrink-0 text-dim">
                    <ConnectionIcon renderer={workspace.protocol(row.protocol)?.renderer} />
                  </span>
                  <span class="max-w-[65%] shrink-0 truncate pl-1 font-condensed">{row.name}</span>
                  {#if row.detail}
                    <span class="ml-1 min-w-0 truncate font-mono text-xs text-dim"
                      >{row.detail}</span
                    >
                  {/if}
                  {#if link}<span class="ml-auto pl-2"><LinkBadge {link} /></span>{/if}
                  <span class="shrink-0 pl-2 text-xs text-dim tabular" class:ml-auto={!link}>
                    {openedAgo(row.opened, recent.now)}
                  </span>
                </span>
                <EnvRail env={row.env} orientation="vertical" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>
{/if}

{#if menu}
  <Menu items={menuItems(menu.row)} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
