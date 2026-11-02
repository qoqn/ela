<script lang="ts">
  import { Button, Dialog, TextField } from '../../ui';
  import { vault } from './vault.svelte';

  let { reason, creating }: { reason: string; creating: boolean } = $props();

  // The master password goes to Rust once and isn't kept here (ADR-0002).
  let master = $state('');
  let repeated = $state('');
  let error = $state('');
  let mismatch = $state('');
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    mismatch = creating && master !== repeated ? 'The two passwords are different.' : '';
    if (mismatch) return;
    busy = true;
    try {
      if (creating) await vault.create(master);
      else await vault.unlock(master);
      master = '';
      repeated = '';
      vault.answer(true);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  title={creating ? 'Create the Vault' : 'Unlock the Vault'}
  onclose={() => vault.answer(false)}
  width="44ch"
>
  <form id="vault-form" class="space-y-3" onsubmit={submit}>
    {#if reason}<p class="text-sm text-dim">{reason}</p>{/if}
    <label class="block text-sm text-dim">
      Master password
      <TextField
        class="mt-1 text-base text-text"
        type="password"
        data-autofocus
        bind:value={master}
        {error}
      />
    </label>
    {#if creating}
      <label class="block text-sm text-dim">
        Repeat master password
        <TextField
          class="mt-1 text-base text-text"
          type="password"
          bind:value={repeated}
          error={mismatch}
        />
      </label>
      <p class="text-sm text-dim">
        Ela can't recover this password. If you forget it, the passwords in the Vault are lost.
      </p>
    {/if}
  </form>
  {#snippet footer()}
    <Button type="button" onclick={() => vault.answer(false)}>Cancel</Button>
    <Button variant="primary" type="submit" form="vault-form" disabled={busy}>
      {creating ? 'Create Vault' : 'Unlock'}
    </Button>
  {/snippet}
</Dialog>
