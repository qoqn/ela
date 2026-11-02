<script lang="ts">
  import type { PromptOf } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { Button, Checkbox, TextField } from '../../ui';
  import { vault } from '../vault/vault.svelte';
  import { passwordCopy, passwordNote, signInAs } from './copy';

  let { tab, prompt }: { tab: SessionTab; prompt: PromptOf<'password'> } = $props();

  // The password goes to Rust once and isn't kept here (ADR-0002).
  let password = $state('');
  let remember = $state(false);
  // Each wrong answer brings a new prompt, and only that one may be answered again.
  let answeredId = $state<number | null>(null);
  const answered = $derived(answeredId === prompt.id);

  const copy = $derived(passwordCopy(prompt.jump_host, tab.name, prompt.tried));
  const note = $derived(prompt.note ? passwordNote(prompt.note, prompt.jump_host, tab.name) : '');
  const formId = `password-${crypto.randomUUID()}`;
  let field = $state<HTMLInputElement>();

  // Again after a wrong answer, which brings a new prompt.
  $effect(() => {
    void prompt;
    field?.focus();
  });

  // Ticked only when the Vault is open, so remembering never brings up a dialog unasked.
  $effect(() => {
    if (!prompt.can_remember) return;
    void vault.refresh().then((info) => (remember = info?.state === 'unlocked'));
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!password || answered) return;
    const keep = prompt.can_remember && remember;
    if (keep && !(await vault.ensureOpen(copy.why))) return;
    const typed = password;
    password = '';
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'password', password: typed, remember: keep });
  }
</script>

<div class="absolute inset-0 z-20 grid place-items-center bg-screen/60 p-4">
  <div
    role="dialog"
    aria-labelledby="password-title-{tab.key}"
    aria-describedby="password-detail-{tab.key}"
    class="w-full max-w-[60ch] rounded-md border border-line bg-panel p-4 shadow-2"
  >
    <h2 id="password-title-{tab.key}" class="font-medium text-label">{copy.title}</h2>
    <p id="password-detail-{tab.key}" class="mt-1 text-sm text-dim">{copy.detail}</p>
    <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 text-sm">
      <dt class="text-dim">Sign in as</dt>
      <dd class="font-mono break-all text-text selectable">
        {signInAs(prompt.username, prompt.host, prompt.port)}
      </dd>
    </dl>
    <form id={formId} class="mt-3" onsubmit={submit}>
      <label class="block text-sm text-dim">
        Password
        <TextField
          class="mt-1 text-base text-text"
          type="password"
          error={note}
          bind:ref={field}
          bind:value={password}
        />
      </label>
      {#if prompt.can_remember}
        <Checkbox class="mt-2" bind:checked={remember}>Remember in the Vault</Checkbox>
      {/if}
    </form>
    <div class="mt-4 flex justify-end gap-2">
      <Button onclick={() => tab.cancel()}>Cancel</Button>
      <Button variant="primary" type="submit" form={formId} disabled={answered || !password}>
        Sign in
      </Button>
    </div>
  </div>
</div>
