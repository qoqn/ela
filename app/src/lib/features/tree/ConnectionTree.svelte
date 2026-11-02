<script lang="ts">
  import { tick } from 'svelte';
  import { linkText } from '../sessions/link';
  import { workspace } from '../../sessions.svelte';
  import { Button, EmptyState, ENV_NAME, Menu, Tree, type MenuItem, type TreeRow } from '../../ui';
  import { saved } from './saved.svelte';
  import { canMoveTo, treeStep, type Row } from './tree';

  const rows = $derived(saved.rows);

  const treeRows = $derived(
    rows.map((row): TreeRow => {
      const link = row.item.kind === 'connection' ? workspace.linkOf(row.item.id) : undefined;
      const parts = [
        row.name,
        ENV_NAME[row.env],
        ...(link ? [linkText(link)] : []),
        ...row.tags.map((tag) => `#${tag}`),
      ];
      return {
        key: row.key,
        label: row.name,
        detail: row.detail,
        level: row.level,
        expandable: row.expandable,
        expanded: row.expanded,
        env: row.env,
        renderer: row.protocol ? workspace.protocol(row.protocol)?.renderer : undefined,
        link,
        description: parts.join(', '),
      };
    }),
  );

  const rowOf = (key: string | null) => rows.find((row) => row.key === key);
  const itemOf = (key: string) => {
    const [kind, id] = key.split(':');
    return { kind, id } as Row['item'];
  };

  let tree = $state<HTMLElement>();
  let menu = $state<{ row: Row | null; x: number; y: number } | null>(null);
  let renameText = $state('');
  let showProblems = $state(false);

  saved.load();

  // No active row means the top of the tree, where new items go. When the active row goes
  // away, as after Delete, the row now in its place takes over instead.
  let lastIndex = 0;
  $effect(() => {
    if (!saved.active) return;
    const index = rows.findIndex((row) => row.key === saved.active);
    if (index >= 0) lastIndex = index;
    else saved.active = rows[Math.min(lastIndex, rows.length - 1)]?.key ?? null;
  });

  function focusTree() {
    requestAnimationFrame(() => tree?.focus());
  }
  saved.focusTree = focusTree;

  function open(row: Row) {
    if (row.item.kind === 'connection') void saved.connect(row.item.id);
    else saved.setExpanded(row.item.id, !row.expanded);
  }

  function startRename(row: Row) {
    saved.renaming = row.key;
  }

  $effect(() => {
    const row = rowOf(saved.renaming);
    if (row) renameText = row.name;
  });

  function menuItems(row: Row | null): MenuItem[] {
    if (!row)
      return [
        { label: 'New connection', keys: 'Ctrl+N', run: () => saved.newConnection() },
        { label: 'New folder', keys: 'Ctrl+Shift+N', run: () => saved.newFolder() },
      ];
    const common: MenuItem[] = [
      { label: 'Edit', keys: 'Alt+Enter', run: () => saved.edit(row.item) },
      { label: 'Rename', keys: 'F2', run: () => startRename(row) },
      {
        label: 'Move to…',
        keys: 'Ctrl+M',
        run: () => (saved.moving = { item: row.item, name: row.name }),
      },
      {
        label: 'Move to Trash',
        keys: 'Delete',
        run: () => void saved.trash(row.item).then(focusTree),
      },
    ];
    if (row.item.kind === 'connection')
      return [{ label: 'Connect', keys: 'Enter', run: () => open(row) }, ...common];
    const { id } = row.item;
    return [
      { label: 'New connection here', run: () => saved.newConnection(id) },
      { label: 'New folder here', run: () => saved.newFolder(id) },
      ...common,
    ];
  }

  async function typeIntoSearch(text: string) {
    saved.search(text);
    await tick();
    saved.focusSearch();
  }

  function onKeydown(e: KeyboardEvent) {
    const row = rowOf(saved.active);
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey;

    const step = plain && !e.shiftKey ? treeStep(e.key, rows, saved.active) : null;
    if (step) {
      if (step.active) saved.active = step.active;
      if (step.expand) saved.setExpanded(step.expand, true);
      if (step.collapse) saved.setExpanded(step.collapse, false);
    } else if (e.key === 'Enter' && e.altKey) {
      if (row) saved.edit(row.item);
    } else if (e.key === 'Enter' && plain) {
      if (row) open(row);
    } else if (e.key === 'F2' && plain) {
      if (row) startRename(row);
    } else if (e.key === 'Delete' && plain) {
      if (row) void saved.trash(row.item);
    } else if (e.ctrlKey && e.key.toLowerCase() === 'm') {
      if (row) saved.moving = { item: row.item, name: row.name };
    } else if (e.ctrlKey && e.key.toLowerCase() === 'n') {
      if (e.shiftKey) saved.newFolder();
      else saved.newConnection();
    } else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      const box = (
        row ? document.getElementById(`tree-${row.key}`) : tree
      )?.getBoundingClientRect();
      menu = {
        row: row ?? null,
        x: (box?.left ?? 0) + 24,
        y: row ? (box?.bottom ?? 0) : (box?.top ?? 0),
      };
    } else if (e.key === 'Escape' && saved.filter) {
      saved.search('');
    } else if (e.key === 'Escape' && saved.active) {
      saved.active = null;
    } else if (e.key === 'Backspace' && plain && saved.filter) {
      typeIntoSearch(saved.filter.slice(0, -1));
    } else if (e.key.length === 1 && plain && (e.key !== ' ' || saved.filter)) {
      typeIntoSearch(saved.filter + e.key);
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  function onRenameKeydown(e: KeyboardEvent, row: TreeRow) {
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      void saved.rename(itemOf(row.key), renameText);
      focusTree();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      saved.renaming = null;
      focusTree();
    }
  }

  const canDrop = (source: string, target: string | null) =>
    canMoveTo(itemOf(source), target ? itemOf(target).id : null, {
      connections: saved.connections,
      folders: saved.folders,
    });

  const focusInput = (element: HTMLInputElement) => {
    element.focus();
    element.select();
  };
</script>

<section class="flex min-h-0 flex-1 flex-col pt-1" aria-label="Connection tree">
  {#if saved.loadError}
    <p class="mx-2 text-sm text-danger-text" role="alert">
      Saved connections can't be opened: {saved.loadError}
    </p>
  {:else if !saved.connections.length && !saved.folders.length}
    <EmptyState
      title="No connections yet."
      detail="Save the machines you use, or bring them in from your SSH config."
    >
      {#snippet actions()}
        <Button size="sm" variant="primary" onclick={() => saved.newConnection()}
          >New connection</Button
        >
        <Button size="sm" onclick={() => (saved.askingImport = true)}>Import SSH hosts</Button>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="relative min-h-0 flex-1">
      <Tree
        bind:ref={tree}
        rows={treeRows}
        active={saved.active}
        label="Connections"
        editing={saved.renaming}
        onselect={(key) => (saved.active = key)}
        ondeselect={() => (saved.active = null)}
        ontoggle={(key) => {
          const row = rowOf(key);
          if (row) saved.setExpanded(row.item.id, !row.expanded);
        }}
        onopen={(key) => {
          const row = rowOf(key);
          if (row) open(row);
        }}
        onmenu={(key, x, y) => {
          const row = key === null ? null : rowOf(key);
          if (row !== undefined) menu = { row, x, y };
        }}
        onkeydown={onKeydown}
        {canDrop}
        ondrop={(source, target) =>
          void saved.drop(itemOf(source), target ? itemOf(target).id : null)}
      >
        {#snippet editor(row)}
          <input
            use:focusInput
            bind:value={renameText}
            aria-label="Name"
            spellcheck="false"
            autocomplete="off"
            class="ml-1 h-5 min-w-0 flex-1 rounded-sm border border-signal bg-chassis px-1 outline-none"
            onkeydown={(e) => onRenameKeydown(e, row)}
            onblur={() => {
              if (saved.renaming === row.key) void saved.rename(itemOf(row.key), renameText);
            }}
            onmousedown={(e) => e.stopPropagation()}
            ondblclick={(e) => e.stopPropagation()}
          />
        {/snippet}
      </Tree>
      {#if !rows.length}
        <!-- Over the tree, which still takes a drop to the top. -->
        <div class="absolute inset-x-0 top-0">
          <EmptyState title="Nothing matches “{saved.filter.trim()}”." live />
        </div>
      {/if}
    </div>
  {/if}

  {#if saved.problems.length}
    <div class="shrink-0 border-t border-line px-2 py-1 text-sm">
      <button
        class="text-left text-danger-text underline-offset-2 hover:underline"
        onclick={() => (showProblems = !showProblems)}
      >
        {saved.problems.length === 1
          ? "1 saved file can't be read."
          : `${saved.problems.length} saved files can't be read.`}
      </button>
      {#if showProblems}
        <ul class="mt-1 max-h-32 overflow-y-auto">
          {#each saved.problems as problem (problem.path)}
            <li class="mb-1">
              <code class="block font-mono text-xs break-all text-text selectable"
                >{problem.path}</code
              >
              <span class="text-dim">{problem.message}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>

{#if menu}
  <Menu items={menuItems(menu.row)} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
