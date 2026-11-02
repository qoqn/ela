<script lang="ts">
  import { Check } from '@lucide/svelte';
  import Kbd from './Kbd.svelte';

  export type MenuItem = {
    label: string;
    keys?: string;
    disabled?: boolean;
    checked?: boolean;
    run: () => void;
  };

  let { items, x, y, onclose }: { items: MenuItem[]; x: number; y: number; onclose: () => void } =
    $props();

  let box = $state<HTMLElement>();
  const enabled = $derived(items.flatMap((item, index) => (item.disabled ? [] : [index])));
  let selected = $derived(enabled[0] ?? -1);

  $effect(() => {
    const previous = document.activeElement as HTMLElement | null;
    box?.focus();
    return () => previous?.focus();
  });

  function run(item: MenuItem) {
    if (item.disabled) return;
    onclose();
    // After focus has returned, so the item can move it elsewhere.
    requestAnimationFrame(item.run);
  }

  function step(by: number) {
    if (!enabled.length) return;
    const at = enabled.indexOf(selected);
    const next = at === -1 ? (by > 0 ? 0 : enabled.length - 1) : at + by;
    selected = enabled[(next + enabled.length) % enabled.length];
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' || e.key === 'Tab') onclose();
    else if (e.key === 'ArrowDown') step(1);
    else if (e.key === 'ArrowUp') step(-1);
    else if (e.key === 'Home') selected = enabled[0] ?? -1;
    else if (e.key === 'End') selected = enabled[enabled.length - 1] ?? -1;
    else if (e.key === 'Enter' && items[selected]) run(items[selected]);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-50"
  onmousedown={(e) => e.target === e.currentTarget && onclose()}
  oncontextmenu={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <div
    bind:this={box}
    role="menu"
    tabindex="-1"
    aria-activedescendant={selected >= 0 ? `menu-item-${selected}` : undefined}
    class="fixed min-w-48 rounded-md border border-line bg-panel py-1 shadow-2 outline-none"
    style:left="min({x}px, calc(100vw - 14rem))"
    style:top="min({y}px, calc(100vh - {items.length * 1.75 + 1}rem))"
    onkeydown={onKeydown}
  >
    {#each items as item, index (item.label)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        id="menu-item-{index}"
        role="menuitem"
        tabindex="-1"
        aria-disabled={item.disabled}
        class="flex h-7 cursor-default items-center justify-between gap-6 px-3 text-sm"
        class:bg-raised={index === selected}
        class:opacity-40={item.disabled}
        onmousemove={() => (selected = item.disabled ? -1 : index)}
        onclick={() => run(item)}
      >
        <span class="flex items-center gap-2">
          {#if item.checked !== undefined}
            <span class="size-3.5"
              >{#if item.checked}<Check size={14} />{/if}</span
            >
          {/if}
          {item.label}
        </span>
        {#if item.keys}<Kbd keys={item.keys} />{/if}
      </div>
    {/each}
  </div>
</div>
