<script lang="ts">
  import type { LinkState } from './types';

  let { state, activity = 0 }: { state: LinkState; activity?: number } = $props();

  const color: Record<LinkState, string> = {
    connecting: 'bg-env-staging',
    waiting: 'bg-env-staging',
    reconnecting: 'bg-env-staging',
    connected: 'bg-env-dev',
    closed: 'bg-env-none',
    failed: 'bg-env-prod',
  };
  let led: HTMLSpanElement;

  $effect(() => {
    if (activity && state === 'connected') {
      led.animate([{ opacity: 0.25 }, { opacity: 1 }], { duration: 120, easing: 'steps(2)' });
    }
  });
</script>

<span
  bind:this={led}
  aria-hidden="true"
  class="inline-block size-1.75 shrink-0 rounded-full {color[state]}"
  class:led-blink={state === 'connecting'}
  class:led-blink-fast={state === 'reconnecting'}
  style:box-shadow={state === 'connected'
    ? '0 0 6px color-mix(in srgb, var(--color-env-dev) 70%, transparent)'
    : 'none'}
></span>

<style>
  .led-blink {
    animation: blink 1s steps(2, jump-none) infinite;
  }
  .led-blink-fast {
    animation: blink 0.5s steps(2, jump-none) infinite;
  }
  @keyframes blink {
    from {
      opacity: 1;
    }
    to {
      opacity: 0.2;
    }
  }
</style>
