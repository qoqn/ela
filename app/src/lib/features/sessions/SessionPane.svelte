<script lang="ts">
  import type { SessionTab } from '../../sessions.svelte';
  import FramebufferView from './FramebufferView.svelte';
  import SessionRail from './SessionRail.svelte';
  import SessionStateLayer from './SessionStateLayer.svelte';
  import StatsOverlay from './StatsOverlay.svelte';
  import TerminalView from './TerminalView.svelte';

  let { tab, active }: { tab: SessionTab; active: boolean } = $props();
</script>

<div class="flex h-full flex-col" role="tabpanel" aria-label="{tab.name}, {tab.protocol}">
  <SessionRail {tab} focused={active} />
  <div class="relative min-h-0 flex-1 bg-screen">
    <!-- `isolate` keeps the renderer's own z-indexes (xterm uses several) inside this
         box, so state banners and overlays above it stay clickable. -->
    <div class="isolate h-full">
      {#if tab.renderer === 'terminal'}
        <TerminalView {tab} {active} />
      {:else if tab.renderer === 'framebuffer'}
        <FramebufferView {tab} {active} />
      {:else}
        <p class="grid h-full place-items-center text-dim">
          Ela can't display {tab.renderer} sessions yet.
        </p>
      {/if}
    </div>
    <SessionStateLayer {tab} />
    {#if tab.statsOpen && tab.status.state === 'connected'}<StatsOverlay {tab} />{/if}
  </div>
</div>
