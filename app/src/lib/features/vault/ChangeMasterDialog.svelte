<script lang="ts">
  import { Button, Dialog, TextField } from '../../ui';
  import { vault } from './vault.svelte';

  // These go to Rust once and aren't kept here (ADR-0002).
  let current = $state('');
  let replacement = $state('');
  let repeated = $state('');
  let error = $state('');
  let mismatch = $state('');
  let busy = $state(false);

  function close() {
    vault.changing = false;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    mismatch = replacement !== repeated ? 'The two passwords are different.' : '';
    if (mismatch) return;
    busy = true;
    try {
      await vault.changeMaster(current, replacement);
      close();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title="Change the master password" onclose={close} width="44ch">
  <form id="change-master-form" class="space-y-3" onsubmit={submit}>
    <label class="block text-sm text-dim">
      Current master password
      <TextField
        class="mt-1 text-base text-text"
        type="password"
        data-autofocus
        bind:value={current}
      />
    </label>
    <label class="block text-sm text-dim">
      New master password
      <TextField class="mt-1 text-base text-text" type="password" bind:value={replacement} />
    </label>
    <label class="block text-sm text-dim">
      Repeat new master password
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
    <p class="text-sm text-dim">
      Copies of the old Vault file, such as backups, still open with the old password.
    </p>
    {#if error}<p class="text-sm text-danger-text selectable" role="alert">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <Button type="button" onclick={close}>Cancel</Button>
    <Button variant="primary" type="submit" form="change-master-form" disabled={busy}>
      Change password
    </Button>
  {/snippet}
</Dialog>
