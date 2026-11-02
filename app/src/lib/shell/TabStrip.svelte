<script lang="ts">
  import { X } from '@lucide/svelte';
  import { EnvRail, ENV_NAME, Led } from '../ui';
  import { workspace } from '../sessions.svelte';
  import { stateLabel } from '../features/sessions/copy';

  function onWheel(e: WheelEvent) {
    if (Math.abs(e.deltaY) < 1) return;
    workspace.cycle(e.deltaY > 0 ? 1 : -1);
  }
</script>

<div
  class="flex h-8.5 shrink-0 items-stretch overflow-x-auto border-b border-line bg-panel"
  role="tablist"
  aria-label="Sessions"
  onwheel={onWheel}
>
  {#each workspace.tabs as tab (tab.key)}
    {@const selected = tab.key === workspace.activeKey}
    <div
      role="tab"
      tabindex={selected ? 0 : -1}
      aria-selected={selected}
      aria-label="{tab.name}, {ENV_NAME[tab.env]}, {stateLabel(tab.status)}"
      title="{tab.address} ({stateLabel(tab.status)})"
      class="group relative flex max-w-60 min-w-32 cursor-default items-center gap-2 border-r border-line pr-1 pl-3 {selected
        ? 'bg-chassis text-label'
        : 'text-dim hover:bg-raised/50 hover:text-text'}"
      onclick={() => workspace.select(tab.key)}
      onkeydown={(e) => e.key === 'Enter' && workspace.select(tab.key)}
      onauxclick={(e) => e.button === 1 && workspace.closeTab(tab.key)}
    >
      <span class="absolute inset-x-0 top-0"
        ><EnvRail env={tab.env} orientation="top" dimmed={!selected} /></span
      >
      <Led state={tab.linkState} activity={selected ? 0 : tab.activity} />
      <span class="flex-1 truncate font-condensed font-medium">{tab.name}</span>
      <button
        class="grid size-5 place-items-center rounded-sm text-dim opacity-0 group-hover:opacity-100 hover:bg-line hover:text-text focus-visible:opacity-100 {selected
          ? 'opacity-100'
          : ''}"
        aria-label="Close {tab.name}"
        title="Close tab (Ctrl+Shift+W)"
        onclick={(e) => {
          e.stopPropagation();
          workspace.closeTab(tab.key);
        }}><X size={13} /></button
      >
    </div>
  {/each}
</div>
