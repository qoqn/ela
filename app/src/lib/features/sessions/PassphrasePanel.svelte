<script lang="ts">
  import { commands } from '../../bindings';
  import type { PromptOf } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { Button, Checkbox, TextField } from '../../ui';
  import { vault } from '../vault/vault.svelte';
  import { PASSPHRASE_NOTE, passphraseCopy, stepName } from './copy';

  let { tab, prompt }: { tab: SessionTab; prompt: PromptOf<'passphrase'> } = $props();

  // The passphrase goes to Rust once and isn't kept here (ADR-0002).
  let passphrase = $state('');
  let remember = $state(false);
  let vaultLocked = $state(false);
  let notSaved = $state(false);
  // Each wrong answer brings a new prompt, and only that one may be answered again.
  let answeredId = $state<number | null>(null);
  const answered = $derived(answeredId === prompt.id);

  const copy = $derived(passphraseCopy(prompt.key, stepName(prompt, tab.name)));
  const note = $derived(
    notSaved ? PASSPHRASE_NOTE.not_saved : prompt.note ? PASSPHRASE_NOTE[prompt.note] : '',
  );
  const formId = `passphrase-${crypto.randomUUID()}`;
  let field = $state<HTMLInputElement>();

  // Again after a wrong answer, which brings a new prompt.
  $effect(() => {
    void prompt;
    field?.focus();
  });

  // Ticked only when the Vault is open, so remembering never brings up a dialog unasked.
  $effect(() => {
    void vault.refresh().then((info) => {
      remember = info?.state === 'unlocked';
      vaultLocked = info?.state === 'locked';
    });
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!passphrase || answered) return;
    if (remember && !(await vault.ensureOpen(`To remember the passphrase of ${prompt.key}.`))) {
      return;
    }
    const typed = passphrase;
    passphrase = '';
    notSaved = false;
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'passphrase', passphrase: typed, remember });
  }

  async function useVault() {
    if (!(await vault.ensureOpen(`To unlock the key ${prompt.key}.`))) return;
    vaultLocked = false;
    remember = true;
    if ((await commands.vaultCredentialState(prompt.credential)) !== 'available') {
      notSaved = true;
      return;
    }
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'saved_passphrase' });
  }

  function skip() {
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'skip_passphrase' });
  }
</script>

<div class="absolute inset-0 z-20 grid place-items-center bg-screen/60 p-4">
  <div
    role="dialog"
    aria-labelledby="passphrase-title-{tab.key}"
    aria-describedby="passphrase-detail-{tab.key}"
    class="w-full max-w-[60ch] rounded-md border border-line bg-panel p-4 shadow-2"
  >
    <h2 id="passphrase-title-{tab.key}" class="font-medium text-label">{copy.title}</h2>
    <p id="passphrase-detail-{tab.key}" class="mt-1 text-sm text-dim">{copy.detail}</p>
    <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 text-sm">
      <dt class="text-dim">Key</dt>
      <dd class="font-mono break-all text-text selectable">{prompt.key}</dd>
    </dl>
    <form id={formId} class="mt-3" onsubmit={submit}>
      <label class="block text-sm text-dim">
        Passphrase
        <TextField
          class="mt-1 text-base text-text"
          type="password"
          error={note}
          bind:ref={field}
          bind:value={passphrase}
        />
      </label>
      <Checkbox class="mt-2" bind:checked={remember}>Remember in the Vault</Checkbox>
    </form>
    <div class="mt-4 flex flex-wrap items-center gap-2">
      <Button variant="ghost" size="sm" disabled={answered} onclick={skip}>Skip this key</Button>
      {#if vaultLocked}
        <Button variant="ghost" size="sm" disabled={answered} onclick={useVault}>
          Unlock the Vault
        </Button>
      {/if}
      <span class="flex-1"></span>
      <Button onclick={() => tab.cancel()}>Cancel</Button>
      <Button variant="primary" type="submit" form={formId} disabled={answered || !passphrase}>
        Sign in
      </Button>
    </div>
  </div>
</div>
