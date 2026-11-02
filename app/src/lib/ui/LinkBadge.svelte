<script lang="ts">
  import Led from './Led.svelte';
  import type { SessionLink } from './types';

  let { link }: { link: SessionLink } = $props();
</script>

<span class="flex shrink-0 items-center gap-1 text-xs text-dim tabular">
  <Led state={link.state} />
  {#if link.count > 1}<span>{link.count}</span>{/if}
</span>
