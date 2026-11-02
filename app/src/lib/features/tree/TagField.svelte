<script lang="ts">
  import { Chip } from '../../ui';
  import { addTags, allTags } from './tags';
  import { saved } from './saved.svelte';

  let { tags = $bindable([]) }: { tags: string[] } = $props();

  let text = $state('');

  const suggestions = $derived(
    allTags(saved.connections).filter(
      (tag) =>
        !tags.some((chosen) => chosen.toLowerCase() === tag.toLowerCase()) &&
        tag.toLowerCase().startsWith(text.trim().toLowerCase()),
    ),
  );

  function commit() {
    tags = addTags(tags, text);
    text = '';
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.key === 'Enter' && text.trim()) || e.key === ',') {
      e.preventDefault();
      commit();
    } else if (e.key === 'Backspace' && !text) {
      tags = tags.slice(0, -1);
    }
  }
</script>

<div
  class="mt-1 flex min-h-7 flex-wrap items-center gap-1 rounded-sm border border-edge bg-chassis px-1 py-0.5 text-base text-text transition-colors duration-(--motion-fast) focus-within:border-signal"
>
  {#each tags as tag (tag)}
    <Chip label={tag} onremove={() => (tags = tags.filter((other) => other !== tag))} />
  {/each}
  <input
    bind:value={text}
    aria-label="Add a tag"
    spellcheck="false"
    autocomplete="off"
    class="h-5 min-w-16 flex-1 bg-transparent px-1 outline-none"
    onkeydown={onKeydown}
    onblur={commit}
  />
</div>
<!-- One line whatever it holds, so typing doesn't move the fields around it. -->
<div class="mt-1 flex h-5 items-center gap-1 overflow-hidden">
  {#if suggestions.length}
    {#each suggestions as tag (tag)}
      <Chip
        label={tag}
        onclick={() => {
          tags = addTags(tags, tag);
          text = '';
        }}
      />
    {/each}
  {:else}
    <span>Enter or a comma adds a tag.</span>
  {/if}
</div>
