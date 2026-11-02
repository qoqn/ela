<script lang="ts">
  import { Monitor, Server, SquareTerminal } from '@lucide/svelte';
  import type { RendererKind } from '../ipc';

  let { renderer, size = 14 }: { renderer?: RendererKind; size?: number } = $props();
</script>

{#if renderer === 'terminal'}
  <SquareTerminal {size} />
{:else if renderer === 'framebuffer'}
  <Monitor {size} />
{:else}
  <!-- Also a Protocol this version doesn't know, as in a Connection synced from a newer Ela. -->
  <Server {size} />
{/if}
