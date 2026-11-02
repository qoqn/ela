<script lang="ts">
  import { commands } from '../../bindings';
  import type { FieldChange } from '../../ipc';
  import { compareNames } from '../../names';
  import { workspace } from '../../sessions.svelte';
  import { Button, Dialog, ENV_NAME, type Env } from '../../ui';
  import { saved, type Moving } from './saved.svelte';
  import { canMoveTo, folderAndInside, folderChain } from './tree';

  let { moving }: { moving: Moving } = $props();

  const TOP = '';
  const items = $derived({ connections: saved.connections, folders: saved.folders });
  const off = $derived(
    moving.item.kind === 'folder'
      ? folderAndInside(moving.item.id, saved.folders)
      : new Set<string>(),
  );
  const targets = $derived([
    { id: TOP, label: 'Top of the tree' },
    ...saved.folders
      .filter((folder) => !off.has(folder.id))
      .map((folder) => ({
        id: folder.id,
        label: folderChain(folder.id, saved.folders)
          .map((each) => each.name)
          .reverse()
          .join(' / '),
      }))
      .sort((a, b) => compareNames(a.label, b.label)),
  ]);

  // svelte-ignore state_referenced_locally
  let to = $state<string>(moving.dropped?.to ?? TOP);
  // svelte-ignore state_referenced_locally
  let changes = $state<FieldChange[]>(moving.dropped?.changes ?? []);
  let error = $state('');

  const target = $derived(to === TOP ? null : to);
  const canMove = $derived(canMoveTo(moving.item, target, items));

  $effect(() => {
    if (moving.dropped) return;
    const destination = target;
    let current = true;
    commands.moveChanges(moving.item, destination).then(
      (found) => {
        if (current) [changes, error] = [found, ''];
      },
      (e) => {
        if (current) [changes, error] = [[], String(e)];
      },
    );
    return () => {
      current = false;
    };
  });

  const LABEL: Record<string, string> = {
    port: 'Port',
    username: 'Username',
    credential: 'Password',
    environment: 'Environment',
  };

  function labelOf(field: string) {
    if (LABEL[field]) return LABEL[field];
    for (const protocol of workspace.protocols) {
      const described = (protocol.settings_schema as Record<string, { label?: string }> | null)?.[
        field
      ];
      if (described?.label) return described.label;
    }
    return field;
  }

  function shown(change: FieldChange, value: string | null): { text: string; typed: boolean } {
    if (change.field === 'environment')
      return { text: ENV_NAME[(value ?? 'none') as Env], typed: false };
    if (change.field === 'credential')
      return { text: value ? 'Saved in the Vault' : 'None', typed: false };
    if (value === null)
      return { text: change.field === 'port' ? 'Protocol default' : 'Not set', typed: false };
    return { text: value, typed: true };
  }

  function onListKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && canMove && !error) {
      e.preventDefault();
      void saved.move(moving.item, target);
      return;
    }
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const index = targets.findIndex((each) => each.id === to);
    const next = targets[Math.max(0, Math.min(index + step, targets.length - 1))];
    to = next.id;
    document.getElementById(`move-${next.id || 'top'}`)?.scrollIntoView({ block: 'nearest' });
  }

  const toName = $derived(targets.find((each) => each.id === to)?.label ?? '');
</script>

{#snippet value({ text, typed }: { text: string; typed: boolean })}
  <td class="pr-2" class:font-mono={typed} class:text-xs={typed} class:text-dim={!typed}>{text}</td>
{/snippet}

<Dialog title="Move {moving.name}" onclose={() => (saved.moving = null)} width="52ch">
  {#if moving.dropped}
    <p class="text-sm">To {toName}.</p>
  {:else}
    <ul
      role="listbox"
      tabindex="0"
      data-autofocus
      aria-label="Move to"
      aria-activedescendant="move-{to || 'top'}"
      class="max-h-48 overflow-y-auto rounded-sm border border-edge bg-chassis py-1 outline-offset-1"
      onkeydown={onListKeydown}
    >
      {#each targets as each (each.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id="move-{each.id || 'top'}"
          role="option"
          aria-selected={each.id === to}
          class="cursor-default truncate px-2 py-0.5 text-sm"
          class:bg-selected={each.id === to}
          onclick={() => (to = each.id)}
        >
          {each.label}
        </li>
      {/each}
    </ul>
  {/if}

  <div class="mt-3 text-sm">
    {#if error}
      <p class="text-danger-text" role="alert">{error}</p>
    {:else if !canMove}
      <p class="text-dim">It's already there.</p>
    {:else if changes.length}
      <p class="text-dim">
        {moving.item.kind === 'folder'
          ? 'These values change for it and what it passes down:'
          : 'These values change:'}
      </p>
      <table class="mt-1 w-full">
        <tbody>
          {#each changes as change (change.field)}
            <tr class="border-t border-line">
              <td class="py-1 pr-3 text-dim">{labelOf(change.field)}</td>
              {@render value(shown(change, change.before))}
              <td class="pr-2 text-dim" aria-label="becomes">→</td>
              {@render value(shown(change, change.after))}
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="text-dim">Nothing it inherits changes.</p>
    {/if}
  </div>

  {#snippet footer()}
    <Button data-autofocus={moving.dropped ? true : undefined} onclick={() => (saved.moving = null)}
      >Cancel</Button
    >
    <Button
      variant="primary"
      disabled={!canMove || Boolean(error)}
      onclick={() => void saved.move(moving.item, target)}>Move</Button
    >
  {/snippet}
</Dialog>
