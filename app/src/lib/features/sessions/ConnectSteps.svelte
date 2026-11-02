<script lang="ts">
  import { Check, X } from '@lucide/svelte';
  import { Led } from '../../ui';
  import type { Step } from './progress';

  let { steps }: { steps: Step[] } = $props();
  const current = $derived(steps.find((step) => step.mark === 'current')?.text ?? '');
</script>

<ol class="grid gap-1.5 text-sm">
  {#each steps as step, i (i)}
    <li
      class="flex items-center gap-2.5"
      class:text-text={step.mark === 'current'}
      class:text-dim={step.mark === 'done' || step.mark === 'pending'}
      class:text-danger-text={step.mark === 'failed'}
    >
      <span class="grid size-3.5 shrink-0 place-items-center" aria-hidden="true">
        {#if step.mark === 'done'}
          <Check size={14} strokeWidth={2} />
        {:else if step.mark === 'failed'}
          <X size={14} strokeWidth={2} />
        {:else if step.mark === 'current'}
          <Led state="connecting" />
        {:else}
          <span class="size-1.75 rounded-full border border-dim"></span>
        {/if}
      </span>
      <span class="min-w-0 truncate">{step.text}</span>
      <span class="sr-only">
        {{ done: ', done', current: '', pending: ', next', failed: ', failed' }[step.mark]}
      </span>
    </li>
  {/each}
</ol>
<p class="sr-only" aria-live="polite">{current}</p>
