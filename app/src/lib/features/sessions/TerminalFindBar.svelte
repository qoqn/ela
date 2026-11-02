<script lang="ts">
  import { ArrowDown, ArrowUp, CaseSensitive, Regex, X } from '@lucide/svelte';
  import { IconButton, TextField } from '../../ui';
  import { findPatternError, findStatus } from './find';
  import type { TerminalFind } from './find.svelte';

  let {
    find,
    limit,
    onstep,
    onclose,
    ref = $bindable(),
  }: {
    find: TerminalFind;
    limit: number;
    onstep: (forward: boolean) => void;
    onclose: () => void;
    ref?: HTMLInputElement;
  } = $props();

  const error = $derived(findPatternError(find.query, find.regex));
  const status = $derived(error || findStatus(find.query, find.results, limit));
  const stuck = $derived(!find.query || !!error || find.results.count === 0);

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      onstep(!e.shiftKey);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    }
  }
</script>

<div
  role="search"
  aria-label="Find in terminal"
  class="absolute top-2 right-3 z-20 flex items-center gap-1 rounded-md border border-line bg-panel p-1 shadow-2"
>
  <TextField
    bind:value={find.query}
    bind:ref
    mono
    placeholder="Find"
    aria-label="Find in terminal"
    class="w-56"
    {onkeydown}
  />
  <span
    class="w-28 truncate px-1 text-xs tabular {error ? 'text-danger-text' : 'text-dim'}"
    aria-live="polite">{status}</span
  >
  <IconButton
    size="sm"
    label="Match case"
    active={find.matchCase}
    onclick={() => (find.matchCase = !find.matchCase)}
  >
    <CaseSensitive size={15} />
  </IconButton>
  <IconButton
    size="sm"
    label="Use regular expression"
    active={find.regex}
    onclick={() => (find.regex = !find.regex)}
  >
    <Regex size={14} />
  </IconButton>
  <IconButton size="sm" label="Previous match" disabled={stuck} onclick={() => onstep(false)}>
    <ArrowUp size={14} />
  </IconButton>
  <IconButton size="sm" label="Next match" disabled={stuck} onclick={() => onstep(true)}>
    <ArrowDown size={14} />
  </IconButton>
  <IconButton size="sm" label="Close" onclick={onclose}>
    <X size={14} />
  </IconButton>
</div>
