<script lang="ts">
  import { Search } from '@lucide/svelte';
  import { commands } from '../../bindings';
  import { workspace } from '../../sessions.svelte';
  import { shell } from '../../shell/shell.svelte';
  import { Button, Chip, EnvPicker, TextField, type Env } from '../../ui';
  import { saved } from '../tree/saved.svelte';
  import { allTags, filtersByTag, searchIntent, toggleTagFilter } from '../tree/tags';
  import { treeStep } from '../tree/tree';

  let username = $state('');
  let env = $state<Env>('none');
  let error = $state('');
  let input = $state<HTMLInputElement>();
  let focused = $state(false);
  let pointerInside = false;

  const intent = $derived(searchIntent(saved.filter, saved.rows.length));
  const tags = $derived(allTags(saved.connections));

  saved.focusSearch = () => {
    input?.focus();
    input?.setSelectionRange(saved.filter.length, saved.filter.length);
  };

  shell.focusSearch = () => {
    input?.focus();
    input?.select();
  };

  shell.connectQuickly = async (address) => {
    saved.search(address);
    await connectToAddress();
  };

  function clear() {
    saved.search('');
    username = '';
    env = 'none';
    error = '';
  }

  // The Tab asks for a password if the server wants one, so the form has no field for it.
  async function connectToAddress() {
    error = '';
    try {
      const target = await commands.parseQuickConnect(saved.filter);
      workspace.open(
        {
          protocol: target.protocol,
          host: target.host,
          port: target.port,
          username: target.username ?? (username || null),
        },
        { env },
      );
      clear();
    } catch (err) {
      error = String(err);
    }
  }

  function onSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (intent === 'address') {
      void connectToAddress();
    } else if (intent === 'search' && saved.activeRow?.item.kind === 'connection') {
      void saved.connect(saved.activeRow.item.id);
      clear();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const step = treeStep(e.key, saved.rows, saved.active);
      if (step?.active) saved.active = step.active;
    } else if (e.key === 'Escape') {
      if (saved.filter) clear();
      else saved.focusTree();
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  // A click on a swatch or a chip can move focus out of the form before the click lands, and
  // folding then would move it away from under the pointer.
  function onFocusOut(e: FocusEvent) {
    const form = e.currentTarget as HTMLElement;
    if (!pointerInside && !form.contains(e.relatedTarget as Node | null)) focused = false;
  }
</script>

<svelte:window onpointerup={() => (pointerInside = false)} />

<form
  class="space-y-2 px-3 pb-3"
  aria-label="Search or connect"
  onsubmit={onSubmit}
  onfocusin={() => (focused = true)}
  onfocusout={onFocusOut}
  onpointerdown={() => (pointerInside = true)}
>
  <TextField
    bind:ref={input}
    bind:value={() => saved.filter, (text) => saved.search(text)}
    mono
    placeholder="Search or connect"
    aria-label="Search connections, or type an address"
    aria-activedescendant={saved.active ? `tree-${saved.active}` : undefined}
    title="A name, host, username or #tag. Or an address like root@10.0.0.5, or rdp://win-ad."
    {error}
    onkeydown={onKeydown}
    oninput={() => (error = '')}
  >
    {#snippet leading()}<Search size={14} />{/snippet}
  </TextField>
  {#if tags.length && intent !== 'address' && (focused || saved.filter)}
    <div class="flex max-h-11.5 flex-wrap gap-1 overflow-y-auto" role="group" aria-label="Tags">
      {#each tags as tag (tag)}
        <Chip
          label="#{tag}"
          pressed={filtersByTag(saved.filter, tag)}
          onclick={() => saved.search(toggleTagFilter(saved.filter, tag))}
        />
      {/each}
    </div>
  {/if}
  {#if intent === 'address'}
    <TextField bind:value={username} placeholder="Username" aria-label="Username" />
    <EnvPicker bind:value={env} />
    <Button variant="primary" class="w-full">Connect</Button>
    <p class="text-sm text-dim">
      Prefix rdp:// for remote desktops. Ela asks for a password if the server wants one.
    </p>
  {/if}
</form>
