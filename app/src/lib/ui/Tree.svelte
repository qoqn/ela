<script lang="ts" module>
  import type { RendererKind } from '../ipc';
  import type { Env, SessionLink } from './types';

  export type TreeRow = {
    key: string;
    label: string;
    detail?: string;
    level: number;
    expandable: boolean;
    expanded: boolean;
    env: Env;
    renderer?: RendererKind;
    link?: SessionLink;
    description: string;
  };
</script>

<script lang="ts">
  import { combine } from '@atlaskit/pragmatic-drag-and-drop/combine';
  import {
    draggable,
    dropTargetForElements,
    monitorForElements,
  } from '@atlaskit/pragmatic-drag-and-drop/element/adapter';
  import { ChevronDown, ChevronRight, Folder } from '@lucide/svelte';
  import type { Snippet } from 'svelte';
  import { VList } from 'virtua/svelte';
  import ConnectionIcon from './ConnectionIcon.svelte';
  import EnvRail from './EnvRail.svelte';
  import LinkBadge from './LinkBadge.svelte';

  type Props = {
    rows: TreeRow[];
    active: string | null;
    label: string;
    editing?: string | null;
    editor?: Snippet<[TreeRow]>;
    onselect: (key: string) => void;
    ontoggle: (key: string) => void;
    onopen: (key: string) => void;
    ondeselect: () => void;
    onmenu: (key: string | null, x: number, y: number) => void;
    onkeydown: (e: KeyboardEvent) => void;
    canDrop: (source: string, target: string | null) => boolean;
    ondrop: (source: string, target: string | null) => void;
    ref?: HTMLElement;
  };
  let {
    rows,
    active,
    label,
    editing = null,
    editor,
    onselect,
    ontoggle,
    onopen,
    ondeselect,
    onmenu,
    onkeydown,
    canDrop,
    ondrop,
    ref = $bindable(),
  }: Props = $props();

  const ROW_HEIGHT = 26;
  let list = $state<VList<TreeRow>>();
  let dropOn = $state<string | null | undefined>(undefined);

  $effect(() => {
    const index = rows.findIndex((row) => row.key === active);
    if (index >= 0) list?.scrollToIndex(index);
  });

  $effect(() => {
    if (!ref) return;
    return monitorForElements({
      onDragStart: ({ source }) => onselect(String(source.data.key)),
      onDrop: () => (dropOn = undefined),
    });
  });

  const keyFromData = (data: Record<string, unknown>) =>
    data.key === null ? null : String(data.key);

  function targetAttachment(element: HTMLElement, key: string | null) {
    return dropTargetForElements({
      element,
      getData: () => ({ key }),
      canDrop: ({ source }) => canDrop(String(source.data.key), key),
      onDropTargetChange: ({ self, location }) => {
        if (location.current.dropTargets[0]?.element === self.element) dropOn = key;
      },
      onDragLeave: ({ self, location }) => {
        if (!location.current.dropTargets.length && dropOn === key && self.element === element)
          dropOn = undefined;
      },
      onDrop: ({ source, self, location }) => {
        if (location.current.dropTargets[0]?.element !== self.element) return;
        ondrop(String(source.data.key), keyFromData(self.data));
      },
    });
  }

  function rowDragDrop(element: HTMLElement, row: () => TreeRow) {
    const current = row();
    const cleanups = [draggable({ element, getInitialData: () => ({ key: current.key }) })];
    // A Connection isn't a place to drop on; the drop falls through to the tree itself.
    if (current.expandable) cleanups.push(targetAttachment(element, current.key));
    return { destroy: combine(...cleanups) };
  }

  function treeDrop(element: HTMLElement) {
    return { destroy: targetAttachment(element, null) };
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={ref}
  use:treeDrop
  role="tree"
  tabindex="0"
  aria-label={label}
  aria-activedescendant={active ? `tree-${active}` : undefined}
  class="list-focus h-full min-h-0 {dropOn === null ? 'bg-raised' : ''}"
  {onkeydown}
  onmousedown={(e) => {
    if (!(e.target as Element).closest('[role=treeitem]')) ondeselect();
  }}
  oncontextmenu={(e) => {
    if ((e.target as Element).closest('[role=treeitem]')) return;
    e.preventDefault();
    ondeselect();
    onmenu(null, e.clientX, e.clientY);
  }}
>
  <VList
    bind:this={list}
    data={rows}
    getKey={(row) => row.key}
    itemSize={ROW_HEIGHT}
    style="height: 100%;"
  >
    {#snippet children(row: TreeRow)}
      <!-- Focus stays on the tree, so a click must find it rather than the row. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
      <div
        use:rowDragDrop={() => row}
        id="tree-{row.key}"
        role="treeitem"
        aria-label={row.description}
        aria-level={row.level + 1}
        aria-selected={row.key === active}
        data-current={row.key === active || undefined}
        aria-expanded={row.expandable ? row.expanded : undefined}
        title={row.description}
        class="flex cursor-default items-stretch text-left {row.key === active
          ? 'bg-selected'
          : 'hover:bg-raised'} {dropOn === row.key
          ? 'outline-2 -outline-offset-2 outline-signal'
          : ''}"
        style:height="{ROW_HEIGHT}px"
        onmousedown={() => onselect(row.key)}
        onclick={(e) => {
          if (row.expandable && (e.target as Element).closest('[data-chevron]')) ontoggle(row.key);
        }}
        ondblclick={(e) => {
          if (!(e.target as Element).closest('[data-chevron]')) onopen(row.key);
        }}
        oncontextmenu={(e) => {
          e.preventDefault();
          onselect(row.key);
          onmenu(row.key, e.clientX, e.clientY);
        }}
      >
        <span
          class="flex min-w-0 flex-1 items-center gap-1 pr-2"
          style:padding-left="{row.level * 12 + 4}px"
        >
          <span data-chevron class="grid size-4 shrink-0 place-items-center text-dim">
            {#if row.expandable}
              {#if row.expanded}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
            {/if}
          </span>
          <span class="shrink-0 text-dim">
            {#if row.expandable}<Folder size={14} />{:else}<ConnectionIcon
                renderer={row.renderer}
              />{/if}
          </span>
          {#if editing === row.key && editor}
            {@render editor(row)}
          {:else}
            <span class="truncate pl-1 font-condensed" class:text-label={row.key === active}
              >{row.label}</span
            >
            {#if row.detail}<span class="ml-1 truncate font-mono text-xs text-dim"
                >{row.detail}</span
              >{/if}
          {/if}
          {#if row.link}<span class="ml-auto pl-2"><LinkBadge link={row.link} /></span>{/if}
        </span>
        <EnvRail env={row.env} orientation="vertical" />
      </div>
    {/snippet}
  </VList>
</div>
