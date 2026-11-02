<script lang="ts" module>
  export type TabItem = { id: string; label: string; invalid?: boolean };
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = {
    tabs: TabItem[];
    selected: string;
    label: string;
    panel: Snippet<[string]>;
  };
  let { tabs, selected = $bindable(), label, panel }: Props = $props();

  const prefix = `tabs-${crypto.randomUUID()}`;
  let list = $state<HTMLElement>();

  function onKeydown(e: KeyboardEvent) {
    const at = tabs.findIndex((tab) => tab.id === selected);
    const next =
      e.key === 'ArrowRight'
        ? (at + 1) % tabs.length
        : e.key === 'ArrowLeft'
          ? (at - 1 + tabs.length) % tabs.length
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? tabs.length - 1
              : -1;
    if (next < 0) return;
    e.preventDefault();
    selected = tabs[next].id;
    list?.querySelectorAll<HTMLElement>('[role=tab]')[next]?.focus();
  }
</script>

<div
  bind:this={list}
  role="tablist"
  aria-label={label}
  tabindex="-1"
  class="flex gap-4 border-b border-line"
  onkeydown={onKeydown}
>
  {#each tabs as tab (tab.id)}
    {@const active = tab.id === selected}
    <button
      type="button"
      role="tab"
      id="{prefix}-tab-{tab.id}"
      aria-selected={active}
      aria-controls="{prefix}-panel-{tab.id}"
      tabindex={active ? 0 : -1}
      data-row
      class="-mb-px flex h-8 items-center gap-1.5 border-b-2 text-sm transition-colors duration-(--motion-fast) {active
        ? 'border-signal text-text'
        : 'border-transparent text-dim hover:text-text'}"
      onclick={() => (selected = tab.id)}
    >
      {tab.label}
      {#if tab.invalid}
        <span class="size-1.5 rounded-full bg-danger-text" aria-hidden="true"></span>
        <span class="sr-only">(has an error)</span>
      {/if}
    </button>
  {/each}
</div>
<!-- The first panel sets the height and the others scroll inside it, so switching tabs doesn't
     move the buttons below and a long tab can't leave the first one with a gap under it. -->
<div class="relative mt-3 min-h-80">
  {#each tabs as tab, index (tab.id)}
    {@const active = tab.id === selected}
    <div
      role="tabpanel"
      id="{prefix}-panel-{tab.id}"
      aria-labelledby="{prefix}-tab-{tab.id}"
      class={index > 0 ? 'scrolling absolute inset-y-0 left-0 -right-3 overflow-y-scroll pr-1' : ''}
      class:invisible={!active}
      inert={!active}
    >
      {@render panel(tab.id)}
    </div>
  {/each}
</div>

<style>
  /* Always there and 8px wide, in the dialog's padding, so fields keep the width they have on the
     first panel whether or not this panel overflows. */
  .scrolling::-webkit-scrollbar {
    width: 8px;
  }
  .scrolling::-webkit-scrollbar-thumb {
    background: var(--color-edge);
    border-radius: 4px;
  }
</style>
