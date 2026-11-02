<script lang="ts">
  import type { TrashEntry } from '../../ipc';
  import { compareNames } from '../../names';
  import { Button, Dialog, EmptyState } from '../../ui';
  import { saved } from './saved.svelte';
  import { trashDetail, trashSummary } from './tree';

  let confirming = $state(false);
  let busy = $state(false);
  let error = $state('');
  let status = $state('');
  let body = $state<HTMLElement>();

  const items = $derived(
    [...(saved.trashed?.items ?? [])].sort((a, b) => compareNames(a.name, b.name)),
  );
  const problems = $derived(saved.trashed?.problems ?? []);
  const passwords = $derived(saved.trashed?.saved_credentials ?? 0);

  function close() {
    saved.trashOpen = false;
  }

  async function restore(entry: TrashEntry) {
    error = '';
    status = '';
    busy = true;
    try {
      await saved.restoreFromTrash(entry);
      status = `Restored ${entry.name}.`;
    } catch (e) {
      error = `Couldn't restore ${entry.name}: ${e}`;
    } finally {
      busy = false;
      // The button that had focus went with its row.
      requestAnimationFrame(() =>
        body?.closest('[role=dialog]')?.querySelector<HTMLElement>('button')?.focus(),
      );
    }
  }

  async function empty() {
    error = '';
    busy = true;
    try {
      if (await saved.emptyTrash()) close();
    } catch (e) {
      error = `Couldn't empty the Trash: ${e}`;
    } finally {
      busy = false;
    }
  }
</script>

{#if confirming}
  <Dialog title="Empty the Trash?" onclose={() => (confirming = false)}>
    <div class="space-y-3 text-sm">
      <p>
        This permanently removes {trashSummary(items)}{#if passwords}, and {passwords === 1
            ? 'the password saved only for them'
            : `the ${passwords} passwords saved only for them`}{/if}. It can't be undone.
      </p>
      {#if error}<p class="text-danger-text selectable" role="alert">{error}</p>{/if}
    </div>
    {#snippet footer()}
      <Button data-autofocus onclick={() => (confirming = false)}>Cancel</Button>
      <Button variant="danger" disabled={busy} onclick={() => void empty()}>Empty Trash</Button>
    {/snippet}
  </Dialog>
{:else}
  <Dialog title="Trash" onclose={close} width="56ch">
    <div bind:this={body} class="space-y-3 text-sm">
      {#if saved.trashError}
        <p class="text-danger-text selectable" role="alert">{saved.trashError}</p>
      {:else if !items.length && !problems.length}
        <EmptyState
          title="The Trash is empty."
          detail="Deleted connections and folders stay here until you empty it."
        />
      {/if}
      {#if items.length}
        <ul class="max-h-72 overflow-y-auto rounded-sm border border-edge bg-chassis">
          {#each items as entry (entry.item.id)}
            <li class="flex items-center gap-2 border-b border-line py-1 pr-1 pl-2 last:border-b-0">
              <div class="min-w-0 flex-1">
                <p class="truncate text-base" title={entry.name}>{entry.name}</p>
                <p
                  class="truncate text-xs text-dim"
                  class:font-mono={entry.host != null}
                  title={trashDetail(entry)}
                >
                  {trashDetail(entry)}
                </p>
              </div>
              <Button size="sm" disabled={busy} onclick={() => void restore(entry)}>Restore</Button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if status}<p class="text-dim" role="status">{status}</p>{/if}
      {#if error}<p class="text-danger-text selectable" role="alert">{error}</p>{/if}
      {#if problems.length}
        <div>
          <p class="text-danger-text">
            {problems.length === 1
              ? "1 file in the Trash can't be read, so it stays there."
              : `${problems.length} files in the Trash can't be read, so they stay there.`}
          </p>
          <ul class="mt-1 max-h-32 overflow-y-auto">
            {#each problems as problem (problem.path)}
              <li class="mb-1">
                <code class="block font-mono text-xs break-all text-text selectable"
                  >{problem.path}</code
                >
                <span class="text-dim">{problem.message}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
    {#snippet footer()}
      <Button
        disabled={!items.length}
        onclick={() => {
          error = '';
          status = '';
          confirming = true;
        }}>Empty Trash</Button
      >
      <Button data-autofocus onclick={close}>Close</Button>
    {/snippet}
  </Dialog>
{/if}
