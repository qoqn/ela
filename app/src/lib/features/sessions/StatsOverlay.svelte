<script lang="ts">
  import type { SessionTab } from '../../sessions.svelte';

  let { tab }: { tab: SessionTab } = $props();
  const mb = (bytes: number) => (bytes / 1_048_576).toFixed(1);
  const merged = $derived(
    tab.statsRate && tab.statsRate.produced > 0
      ? Math.max(0, 100 - (tab.statsRate.sent / tab.statsRate.produced) * 100)
      : null,
  );
  const rows = $derived.by(() => {
    const r: [string, string][] = [
      ['Drawing with', tab.drawBackend],
      ['Received', `${mb(tab.meter.bytesPerSec)} MB/s`],
    ];
    if (tab.renderer === 'framebuffer') {
      r.push(['Painted', `${tab.meter.paintsPerSec} fps`]);
      if (tab.statsRate)
        r.push(
          ['Source updates', `${tab.statsRate.produced}/s`],
          ['Merged away', `${merged?.toFixed(0) ?? 0}%`],
        );
    } else if (tab.stats) {
      r.push(['Total', `${mb(tab.stats.bytes)} MB`]);
    }
    return r;
  });
</script>

<dl
  class="absolute top-2 right-3 z-10 grid grid-cols-[auto_auto] gap-x-4 rounded-md border border-line bg-panel/95 px-3 py-2 text-sm shadow-2"
  aria-label="Throughput"
>
  {#each rows as [k, v] (k)}
    <dt class="text-dim">{k}</dt>
    <dd class="text-right tabular">{v}</dd>
  {/each}
</dl>
