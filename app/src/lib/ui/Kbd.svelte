<script lang="ts">
  let { keys }: { keys: string } = $props();
  const isMac = typeof navigator !== 'undefined' && /Mac/.test(navigator.platform);
  const parts = $derived(keys.split('+').map((k) => (isMac && k === 'Ctrl' ? '⌘' : k)));
</script>

<span class="inline-flex gap-0.5 text-xs text-dim">
  {#each parts as k, i (i)}
    <kbd class="rounded-sm border border-line bg-panel px-1 font-sans leading-4">{k}</kbd>
  {/each}
</span>
