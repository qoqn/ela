<script lang="ts">
  import { ChevronDown, ChevronRight, RefreshCw, Search, Trash2 } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { VList } from 'virtua/svelte';
  import { Button, Dialog, EmptyState, IconButton, Panel, TextField } from '../../ui';
  import { identityLabel } from '../sessions/copy';
  import { knownHosts } from './knownHosts.svelte';
  import { knownHostRows, shownName } from './rows';

  const TITLES = { ssh_keys: 'SSH keys', certificates: 'Certificates' };
  const EMPTY = {
    ssh_keys: 'No saved keys. Ela saves one when you trust a server.',
    certificates: 'No saved certificates. Ela saves one when you trust a server.',
  };

  onMount(() => void knownHosts.load());

  const rows = $derived(knownHostRows(knownHosts.sections, knownHosts.filter, knownHosts.closed));
  const filtering = $derived(!!knownHosts.filter.trim());
  const removing = $derived(knownHosts.removing);
  const identity = $derived(removing?.section.store === 'certificates' ? 'certificate' : 'key');
</script>

<Panel title="Known Hosts">
  {#snippet actions()}
    <IconButton label="Refresh" onclick={() => void knownHosts.load()}>
      <RefreshCw size={15} />
    </IconButton>
  {/snippet}

  <div class="px-3 pb-2">
    <TextField
      bind:value={knownHosts.filter}
      placeholder="Filter by name or fingerprint"
      aria-label="Filter known hosts"
    >
      {#snippet leading()}<Search size={14} />{/snippet}
    </TextField>
  </div>

  {#if knownHosts.notice}
    <p class="mx-3 mb-2 text-sm text-dim" role="status">{knownHosts.notice}</p>
  {/if}

  {#if knownHosts.loadError}
    <p class="mx-3 text-sm text-danger-text" role="alert">{knownHosts.loadError}</p>
  {:else if knownHosts.loaded && !rows.length}
    <EmptyState title="Nothing matches “{knownHosts.filter.trim()}”." live />
  {:else}
    <div class="min-h-0 flex-1">
      <VList data={rows} getKey={(row) => row.key}>
        {#snippet children(row)}
          {#if row.kind === 'heading'}
            <div class="pb-1 {row.key === rows[0].key ? '' : 'border-t border-line'}">
              <h3>
                <button
                  data-row
                  class="flex h-7 w-full items-center gap-1 pr-3 pl-1.5 text-left text-sm font-semibold text-text enabled:hover:bg-raised"
                  aria-expanded={row.open}
                  disabled={filtering}
                  onclick={() => knownHosts.toggle(row.section.store)}
                >
                  <span class="text-dim">
                    {#if row.open}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
                  </span>
                  <span class="flex-1">{TITLES[row.section.store]}</span>
                  <span class="text-xs font-normal text-dim tabular">{row.count}</span>
                </button>
              </h3>
              {#if row.open}
                <p
                  class="truncate px-3 font-mono text-xs text-dim selectable"
                  title={row.section.path}
                >
                  {row.section.path}
                </p>
                {#if row.section.problem}
                  <p class="mt-1 pr-3 pl-6 text-sm text-danger-text selectable" role="alert">
                    {row.section.problem}
                  </p>
                {:else if !row.section.entries.length}
                  <p class="mt-1 pr-3 pl-6 text-sm text-dim">{EMPTY[row.section.store]}</p>
                {/if}
                {#if row.section.other_lines && !filtering}
                  <p class="mt-1 pr-3 pl-6 text-sm text-dim">
                    Left as they are: {row.section.other_lines}
                    {row.section.other_lines === 1 ? 'line' : 'lines'} that Ela can't manage here, such
                    as revoked keys.
                  </p>
                {/if}
              {/if}
            </div>
          {:else}
            <div class="flex h-10.5 items-center gap-1 pr-1 pl-3 hover:bg-raised">
              <div class="min-w-0 flex-1">
                <p class="flex min-w-0 items-baseline gap-2 text-sm" title={row.entry.name}>
                  <span
                    class="truncate font-mono selectable {row.entry.hashed && !row.entry.known_as
                      ? 'text-dim'
                      : 'text-text'}"
                  >
                    {shownName(row.entry)}
                  </span>
                  {#if row.entry.hashed && row.entry.known_as}
                    <span class="shrink-0 text-xs text-dim">hashed</span>
                  {/if}
                </p>
                <p class="truncate font-mono text-xs text-dim" title={row.entry.fingerprint}>
                  {identityLabel(row.entry.algorithm)}
                  {row.entry.fingerprint}
                </p>
              </div>
              <IconButton
                label="Remove {shownName(row.entry)}"
                onclick={() => knownHosts.ask(row.section, row.entry)}
              >
                <Trash2 size={14} />
              </IconButton>
            </div>
          {/if}
        {/snippet}
      </VList>
    </div>
  {/if}
</Panel>

{#if removing}
  <Dialog title="Remove this {identity}?" onclose={() => knownHosts.cancel()} width="56ch">
    <div class="space-y-3 text-sm">
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 font-mono text-xs">
        <dt class="text-dim">Server</dt>
        <dd class="break-all selectable">{shownName(removing.entry)}</dd>
        {#if removing.entry.hashed}
          <dt class="text-dim">Saved as</dt>
          <dd class="break-all selectable">{removing.entry.name}</dd>
        {/if}
        <dt class="text-dim">{identityLabel(removing.entry.algorithm)}</dt>
        <dd class="break-all selectable">{removing.entry.fingerprint}</dd>
      </dl>
      <p>
        The next connection to this server will ask you to trust it again, without warning that
        anything changed.
        {#if removing.section.store === 'ssh_keys'}
          ssh and other tools use this file too, so they will ask as well.
        {/if}
        The previous file stays next to it as known_hosts.old.
      </p>
      {#if knownHosts.removeError}
        <p class="text-danger-text selectable" role="alert">{knownHosts.removeError}</p>
      {/if}
    </div>
    {#snippet footer()}
      <Button data-autofocus onclick={() => knownHosts.cancel()}>Cancel</Button>
      <Button variant="danger" disabled={knownHosts.busy} onclick={() => void knownHosts.confirm()}>
        Remove {identity}
      </Button>
    {/snippet}
  </Dialog>
{/if}
