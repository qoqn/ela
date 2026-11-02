<script lang="ts">
  import { untrack } from 'svelte';
  import type { PromptOf } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { Button, TextField } from '../../ui';
  import { QUESTIONS_REFUSED, questionLabel, questionsCopy, signInAs, stepName } from './copy';

  type Prompt = PromptOf<'questions'>;
  let { tab, prompt }: { tab: SessionTab; prompt: Prompt } = $props();

  // An answer may be a password or a one-time code, so it goes to Rust once and isn't kept
  // here (ADR-0002).
  // The effect below starts every later round afresh.
  let answers = $state(untrack(() => blank(prompt)));
  // Each round brings a new prompt, and only that one may be answered.
  let answeredId = $state<number | null>(null);
  const answered = $derived(answeredId === prompt.id);

  const copy = $derived(questionsCopy(stepName(prompt, tab.name), prompt.questions.length));
  const serverText = $derived([prompt.name, prompt.instructions].filter((t) => t.trim()));
  const formId = `questions-${crypto.randomUUID()}`;
  let first = $state<HTMLInputElement>();

  function blank(prompt: Prompt) {
    return prompt.questions.map(() => '');
  }

  // Again for each round, which brings a new prompt.
  $effect(() => {
    answers = blank(prompt);
    first?.focus();
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (answered) return;
    const typed = answers;
    answers = blank(prompt);
    answeredId = prompt.id;
    tab.answer(prompt.id, { type: 'answers', answers: typed });
  }
</script>

<div class="absolute inset-0 z-20 grid place-items-center bg-screen/60 p-4">
  <div
    role="dialog"
    aria-labelledby="questions-title-{tab.key}"
    aria-describedby="questions-detail-{tab.key}"
    class="w-full max-w-[60ch] rounded-md border border-line bg-panel p-4 shadow-2"
  >
    <h2 id="questions-title-{tab.key}" class="font-medium text-label">{copy.title}</h2>
    <p id="questions-detail-{tab.key}" class="mt-1 text-sm text-dim">{copy.detail}</p>
    <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
      <dt class="text-dim">Sign in as</dt>
      <dd class="font-mono break-all text-text selectable">
        {signInAs(prompt.username, prompt.host, prompt.port)}
      </dd>
      {#if serverText.length}
        <dt class="text-dim">The server says</dt>
        <dd class="break-words whitespace-pre-wrap text-text selectable">
          {serverText.join('\n')}
        </dd>
      {/if}
    </dl>
    {#if prompt.refused}
      <p role="alert" class="mt-3 text-sm text-danger">{QUESTIONS_REFUSED}</p>
    {/if}
    <form id={formId} class="mt-3 grid gap-3" onsubmit={submit}>
      {#each prompt.questions as question, i (i)}
        <label class="block text-sm text-dim">
          {questionLabel(question.text)}
          {#if i === 0}
            <TextField
              class="mt-1 text-base text-text"
              type={question.echo ? 'text' : 'password'}
              bind:ref={first}
              bind:value={answers[i]}
            />
          {:else}
            <TextField
              class="mt-1 text-base text-text"
              type={question.echo ? 'text' : 'password'}
              bind:value={answers[i]}
            />
          {/if}
        </label>
      {/each}
    </form>
    <div class="mt-4 flex justify-end gap-2">
      <Button onclick={() => tab.cancel()}>Cancel</Button>
      <Button variant="primary" type="submit" form={formId} disabled={answered}>Sign in</Button>
    </div>
  </div>
</div>
