<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    detail?: string;
    hint?: string;
    live?: boolean;
    actions?: Snippet;
  };
  let { title, detail = '', hint = '', live = false, actions }: Props = $props();
</script>

<div
  class="flex min-h-0 flex-1 flex-col items-center px-4 pt-6 pb-4 text-center text-sm"
  aria-live={live ? 'polite' : undefined}
>
  <p class="max-w-full truncate font-medium text-text" {title}>{title}</p>
  {#if detail}<p class="mt-1 max-w-[34ch] text-dim">{detail}</p>{/if}
  {#if actions}
    <div class="mt-3 flex w-full max-w-56 flex-col gap-2">{@render actions()}</div>
  {/if}
  {#if hint}<p class="mt-2 text-xs text-dim">{hint}</p>{/if}
</div>
