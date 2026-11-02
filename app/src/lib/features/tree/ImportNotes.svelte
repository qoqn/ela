<script lang="ts">
  import { Button, Dialog } from '../../ui';
  import type { ImportResult } from './saved.svelte';
  import { saved } from './saved.svelte';

  let { result }: { result: ImportResult } = $props();
</script>

<Dialog title={result.title} onclose={() => (saved.imported = null)} width="64ch">
  {#if result.detail}<p class="text-sm text-dim">{result.detail}</p>{/if}
  <p class="mt-2 text-sm text-dim">Some of ~/.ssh/config was left out:</p>
  <ul class="mt-2 space-y-1 font-mono text-xs text-text selectable">
    {#each result.notes as note (note)}<li>{note}</li>{/each}
  </ul>
  {#snippet footer()}
    <Button data-autofocus variant="primary" type="button" onclick={() => (saved.imported = null)}>
      Close
    </Button>
  {/snippet}
</Dialog>
