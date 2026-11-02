<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { PromptOf } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { Button, TextField } from '../../ui';
  import { vault } from '../vault/vault.svelte';
  import { vaultPromptCopy } from './copy';

  let { tab, prompt }: { tab: SessionTab; prompt: PromptOf<'vault'> } = $props();

  // The master password goes to Rust once and isn't kept here (ADR-0002).
  let master = $state('');
  // A wrong password brings a new prompt, and only that one may be answered again.
  let answeredId = $state<number | null>(null);
  const answered = $derived(answeredId === prompt.id);
  const copy = $derived(vaultPromptCopy(prompt.for_jump_host, tab.name));
  const formId = `vault-${crypto.randomUUID()}`;
  let field = $state<HTMLInputElement>();

  $effect(() => {
    void prompt;
    field?.focus();
  });

  // Rust unlocked it, so the rest of the window learns once the Tab moves on.
  onDestroy(() => void vault.refresh());

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!master || answered) return;
    const typed = master;
    master = '';
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'vault', master: typed });
  }

  function without() {
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'without_vault' });
  }
</script>

<div class="absolute inset-0 z-20 grid place-items-center bg-screen/60 p-4">
  <div
    role="dialog"
    aria-labelledby="vault-title-{tab.key}"
    aria-describedby="vault-detail-{tab.key}"
    class="w-full max-w-[60ch] rounded-md border border-line bg-panel p-4 shadow-2"
  >
    <h2 id="vault-title-{tab.key}" class="font-medium text-label">{copy.title}</h2>
    <p id="vault-detail-{tab.key}" class="mt-1 text-sm text-dim">{copy.detail}</p>
    <form id={formId} class="mt-3" onsubmit={submit}>
      <label class="block text-sm text-dim">
        Master password
        <TextField
          class="mt-1 text-base text-text"
          type="password"
          error={prompt.wrong ? copy.wrong : ''}
          bind:ref={field}
          bind:value={master}
        />
      </label>
    </form>
    <div class="mt-4 flex items-center justify-between gap-2">
      <Button variant="ghost" size="sm" disabled={answered} onclick={without}>
        Connect without it
      </Button>
      <div class="flex gap-2">
        <Button onclick={() => tab.cancel()}>Cancel</Button>
        <Button variant="primary" type="submit" form={formId} disabled={answered || !master}>
          Unlock
        </Button>
      </div>
    </div>
  </div>
</div>
