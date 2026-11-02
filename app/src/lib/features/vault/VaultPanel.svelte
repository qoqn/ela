<script lang="ts">
  import { Trash2 } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { shell } from '../../shell/shell.svelte';
  import { Button, Dialog, IconButton, Panel, SectionHeader, Separator } from '../../ui';
  import { shortFingerprint } from './keys';
  import ChangeMasterDialog from './ChangeMasterDialog.svelte';
  import { idleLabel } from './idle';
  import { vault } from './vault.svelte';

  const REFRESH_MS = 5000;

  // The Vault can lock by itself while this is showing.
  const refresh = () => void vault.refresh().then(() => vault.loadKeys());
  onMount(() => {
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => {
      clearInterval(timer);
      vault.notice = '';
      vault.keysNotice = '';
    };
  });
</script>

<Panel title="Vault">
  <div class="space-y-3 px-3 pb-3 text-sm text-dim" aria-live="polite">
    {#if vault.loadError}
      <p class="text-danger-text" role="alert">{vault.loadError}</p>
    {:else if vault.info?.state === 'absent'}
      <p>No Vault yet. Create one to save passwords with your connections.</p>
      <Button variant="primary" class="w-full" onclick={() => vault.ask('')}>Create Vault</Button>
    {:else if vault.info?.state === 'locked'}
      <p>Locked. Connections with a saved password wait until you unlock it.</p>
      <Button variant="primary" class="w-full" onclick={() => vault.ask('')}>Unlock</Button>
      <p>Open sessions stay open while it is locked.</p>
    {:else if vault.info?.state === 'unlocked'}
      <p>
        Unlocked. It locks after {idleLabel(vault.info.idle_minutes)} without use.
        <button
          type="button"
          class="text-text underline underline-offset-2"
          onclick={() => shell.openSettings('vault')}>Change</button
        >
      </p>
      <Button class="w-full" onclick={() => void vault.lock()}>Lock now</Button>
      <p>Open sessions stay open when it locks.</p>
    {/if}
  </div>
  {#if vault.info && vault.info.state !== 'absent'}
    <Separator />
    <div class="space-y-3 px-3 py-3 text-sm text-dim">
      <Button class="w-full" onclick={() => (vault.changing = true)}>Change master password</Button>
      {#if vault.notice}<p role="status">{vault.notice}</p>{/if}
    </div>
  {/if}
  {#if vault.info?.state === 'unlocked'}
    <Separator />
    <SectionHeader title="Key passphrases" />
    {#if vault.keysError}
      <p class="px-3 pb-3 text-sm text-danger-text selectable" role="alert">{vault.keysError}</p>
    {:else if !vault.keys.length}
      <p class="px-3 pb-3 text-sm text-dim">
        None saved. A key's passphrase is saved when you tick “Remember in the Vault”.
      </p>
    {:else}
      <ul class="pb-2" aria-label="Key passphrases">
        {#each vault.keys as key (key.credential)}
          <li class="flex h-10.5 items-center gap-1 pr-1 pl-3 hover:bg-raised">
            <div class="min-w-0 flex-1">
              <p class="truncate font-mono text-sm text-text selectable" title={key.path}>
                {key.path}
              </p>
              <p class="truncate font-mono text-xs text-dim" title={key.fingerprint ?? undefined}>
                {key.fingerprint ? shortFingerprint(key.fingerprint) : 'No public key beside it'}
              </p>
            </div>
            <IconButton
              label="Forget the passphrase of {key.path}"
              onclick={() => vault.askForget(key)}
            >
              <Trash2 size={14} />
            </IconButton>
          </li>
        {/each}
      </ul>
    {/if}
    {#if vault.keysNotice}
      <p class="px-3 pb-3 text-sm text-dim" role="status">{vault.keysNotice}</p>
    {/if}
  {/if}
</Panel>

{#if vault.forgetting}
  {@const key = vault.forgetting}
  <Dialog title="Forget this passphrase?" onclose={() => vault.cancelForget()} width="56ch">
    <div class="space-y-3 text-sm">
      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 font-mono text-xs">
        <dt class="text-dim">Key</dt>
        <dd class="break-all selectable">{key.path}</dd>
        {#if key.fingerprint}
          <dt class="text-dim">Fingerprint</dt>
          <dd class="break-all selectable">{key.fingerprint}</dd>
        {/if}
      </dl>
      <p>
        The next time this key signs in, Ela asks for its passphrase again. The key file stays as it
        is.
      </p>
      {#if vault.forgetError}
        <p class="text-danger-text selectable" role="alert">{vault.forgetError}</p>
      {/if}
    </div>
    {#snippet footer()}
      <Button data-autofocus onclick={() => vault.cancelForget()}>Cancel</Button>
      <Button variant="danger" disabled={vault.busy} onclick={() => void vault.confirmForget()}>
        Forget passphrase
      </Button>
    {/snippet}
  </Dialog>
{/if}

{#if vault.changing}<ChangeMasterDialog />{/if}
