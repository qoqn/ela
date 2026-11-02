<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, action }: { title: string; action?: Snippet } = $props();
</script>

<div class="flex h-7 shrink-0 items-center justify-between gap-1 pr-1.5 pl-3">
  <h3 class="truncate text-sm font-semibold text-dim">{title}</h3>
  {#if action}{@render action()}{/if}
</div>
