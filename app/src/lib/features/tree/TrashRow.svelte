<script lang="ts">
  import { Trash2 } from '@lucide/svelte';
  import { saved } from './saved.svelte';
  import { trashSize } from './tree';

  const inTrash = $derived(trashSize(saved.trashed?.items ?? []));
</script>

<button
  data-row
  class="flex h-7 shrink-0 items-center gap-2 border-t border-line px-3 text-sm text-dim hover:bg-raised hover:text-text"
  onclick={() => saved.openTrash()}
>
  <Trash2 size={14} aria-hidden="true" />
  <span class="flex-1 text-left">Trash</span>
  {#if inTrash}<span aria-label="{inTrash} {inTrash === 1 ? 'item' : 'items'}">{inTrash}</span>{/if}
</button>
