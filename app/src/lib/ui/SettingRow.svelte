<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = { label: string; description?: string; error?: string; children: Snippet };
  let { label, description, error, children }: Props = $props();
</script>

<div class="border-t border-line py-3">
  <div class="flex items-center justify-between gap-6">
    <div class="min-w-0">
      <p>{label}</p>
      {#if description}<p class="mt-0.5 text-sm text-dim">{description}</p>{/if}
    </div>
    <div class="flex shrink-0 items-center gap-1">{@render children()}</div>
  </div>
  {#if error}<p class="mt-1 text-sm text-danger-text selectable" role="alert">{error}</p>{/if}
</div>
