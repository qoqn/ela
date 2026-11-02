<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    actions?: Snippet;
    children: Snippet;
    ref?: HTMLElement;
  };
  let { title, actions, children, ref = $bindable() }: Props = $props();
</script>

<aside
  bind:this={ref}
  class="flex h-full w-full flex-col border-r border-line bg-panel"
  aria-label={title}
>
  <header class="flex h-9 shrink-0 items-center gap-1 pr-1.5 pl-3">
    <h2 class="min-w-0 flex-1 truncate text-sm font-semibold text-text">{title}</h2>
    {#if actions}{@render actions()}{/if}
  </header>
  {@render children()}
</aside>
