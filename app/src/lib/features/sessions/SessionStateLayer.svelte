<script lang="ts">
  import { CircleAlert, ShieldAlert } from '@lucide/svelte';
  import { saved } from '../tree/saved.svelte';
  import { Button } from '../../ui';
  import { promptOf } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { workspace } from '../../sessions.svelte';
  import { copyText } from '../../clipboard';
  import ConnectSteps from './ConnectSteps.svelte';
  import PasswordPanel from './PasswordPanel.svelte';
  import PassphrasePanel from './PassphrasePanel.svelte';
  import QuestionsPanel from './QuestionsPanel.svelte';
  import VaultPromptPanel from './VaultPromptPanel.svelte';
  import { endpoint, errorCopy, identityLabel, stepName, trustCopy } from './copy';
  import { steps } from './progress';
  import { reconnect } from './reconnect.svelte';

  let { tab }: { tab: SessionTab } = $props();
  const busy = $derived(
    ['idle', 'connecting', 'verifying', 'authenticating'].includes(tab.status.state),
  );
  const failure = $derived(
    tab.status.state === 'closed' && tab.status.error
      ? errorCopy(
          tab.status.error,
          tab.name,
          endpoint(tab.host, tab.port ?? workspace.protocol(tab.protocol)?.default_port),
          { host: tab.host, port: tab.port },
        )
      : null,
  );
  const trust = $derived(promptOf(tab.status, 'trust'));
  const passphrase = $derived(promptOf(tab.status, 'passphrase'));
  const password = $derived(promptOf(tab.status, 'password'));
  const questions = $derived(promptOf(tab.status, 'questions'));
  const unlock = $derived(promptOf(tab.status, 'vault'));
  const trustText = $derived(
    trust &&
      trustCopy(
        stepName(trust, tab.name),
        trust.algorithm,
        trust.known !== null,
        trust.known_types,
      ),
  );
  let trustPanel = $state<HTMLElement>();
  // A route through Jump Hosts asks once per host, so an answer only counts for its own prompt.
  let answeredId = $state<number | null>(null);
  const answered = $derived(trust !== null && answeredId === trust.id);

  // Focus starts on the safest action, so a stray Enter can't accept a key.
  $effect(() => {
    trustPanel?.querySelector<HTMLElement>('[data-safest]')?.focus();
  });

  function answerTrust(accept: boolean) {
    if (!trust) return;
    answeredId = trust.id;
    tab.answer(trust.id, { type: 'trust', trust: accept });
  }

  const target = $derived({ name: tab.name, user: tab.user });

  // Fast connections finish before a clock would mean anything.
  const CLOCK_AFTER = 2;
  let seconds = $state(0);
  $effect(() => {
    if (!busy) return;
    const tick = () => (seconds = Math.floor((Date.now() - tab.startedAt) / 1000));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  });

  let copyResult = $state<'idle' | 'copied' | 'failed'>('idle');
  const copyLabel = { idle: 'Copy details', copied: 'Copied', failed: "Couldn't copy" };

  async function copyDetails() {
    if (!failure) return;
    const raw = tab.status.state === 'closed' && tab.status.error ? tab.status.error : null;
    const detail = raw && 'detail' in raw ? raw.detail : '';
    const technical = raw
      ? `\n\n${raw.kind}: ${typeof detail === 'string' ? detail : JSON.stringify(detail)}\n${tab.protocol}://${tab.address}`
      : '';
    const fix = failure.fix ? `\n\n${failure.fix.command}` : '';
    copyResult = (await copyText(`${failure.title}\n${failure.detail}${fix}${technical}`))
      ? 'copied'
      : 'failed';
    setTimeout(() => (copyResult = 'idle'), 2000);
  }
</script>

{#snippet outcome()}
  {#if failure}
    <p class="flex items-center gap-2 font-medium text-danger-text">
      <CircleAlert size={16} class="shrink-0" aria-hidden="true" />{failure.title}
    </p>
  {:else}
    <p class="font-medium">Disconnected</p>
  {/if}
  {#if failure?.detail}
    <p class="mt-0.5 max-w-[72ch] text-sm text-dim selectable">{failure.detail}</p>
  {/if}
  {#if failure?.fix}
    <p class="mt-2 text-sm text-dim">{failure.fix.text}</p>
    <code
      class="mt-1 inline-block rounded-sm bg-chassis px-2 py-1 font-mono text-xs text-text selectable"
      >{failure.fix.command}</code
    >
  {/if}
{/snippet}

{#snippet actions()}
  {#if failure}
    <Button variant="ghost" size="sm" onclick={copyDetails}>{copyLabel[copyResult]}</Button>
  {/if}
  {#if tab.connection}
    <Button size="sm" onclick={() => saved.editById(tab.connection!)}>Edit connection</Button>
  {/if}
  <Button size="sm" onclick={() => workspace.closeTab(tab.key)}>Close tab</Button>
  <Button size="sm" variant="primary" onclick={() => reconnect.start(tab)}>Reconnect</Button>
{/snippet}

<!-- Nothing has been drawn before the Session connects, so the card covers the screen
     instead of dimming it, which also hides the terminal's idle cursor. -->
{#if busy}
  <div class="absolute inset-0 z-10 grid place-items-center bg-screen p-4">
    <div class="w-full max-w-[52ch] rounded-md border border-line bg-panel p-4 shadow-2">
      <ConnectSteps steps={steps(tab.progress, target)} />
      <div class="mt-4 flex items-center justify-between gap-3">
        <span class="text-xs text-dim tabular-nums">
          {seconds >= CLOCK_AFTER ? `${seconds} s` : ''}
        </span>
        <Button size="sm" onclick={() => tab.cancel()}>Cancel</Button>
      </div>
    </div>
  </div>
{:else if trust && trustText}
  <div class="absolute inset-0 z-20 grid place-items-center bg-screen/60 p-4">
    <div
      bind:this={trustPanel}
      role="alertdialog"
      aria-labelledby="trust-title-{tab.key}"
      aria-describedby="trust-detail-{tab.key}"
      class="w-full max-w-[60ch] rounded-md border bg-panel p-4 shadow-2 {trust.known
        ? 'border-danger'
        : 'border-line'}"
    >
      <h2
        id="trust-title-{tab.key}"
        class="flex items-center gap-2 font-medium {trust.known
          ? 'text-danger-text'
          : 'text-label'}"
      >
        {#if trust.known}<ShieldAlert size={16} class="shrink-0" aria-hidden="true" />{/if}
        {trustText.title}
      </h2>
      <p id="trust-detail-{tab.key}" class="mt-1 text-sm text-dim">{trustText.detail}</p>
      <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
        {#if trust.known}
          <dt class="text-dim">Saved</dt>
          <dd class="font-mono break-all text-text selectable">{trust.known}</dd>
          <dt class="text-dim">Now</dt>
        {:else}
          <dt class="text-dim">{identityLabel(trust.algorithm)}</dt>
        {/if}
        <dd class="font-mono break-all text-text selectable">{trust.fingerprint}</dd>
      </dl>
      <div class="mt-4 flex justify-end gap-2">
        <Button data-safest disabled={answered} onclick={() => answerTrust(false)}>Cancel</Button>
        <Button
          variant={trust.known ? 'danger' : 'primary'}
          disabled={answered}
          onclick={() => answerTrust(true)}
        >
          {trustText.action}
        </Button>
      </div>
    </div>
  </div>
{:else if passphrase}
  <PassphrasePanel {tab} prompt={passphrase} />
{:else if password}
  <PasswordPanel {tab} prompt={password} />
{:else if questions}
  <QuestionsPanel {tab} prompt={questions} />
{:else if unlock}
  <VaultPromptPanel {tab} prompt={unlock} />
{:else if tab.status.state === 'closed' && !tab.everConnected}
  <div class="absolute inset-0 z-10 grid place-items-center bg-screen p-4">
    <div
      role={failure ? 'alert' : 'status'}
      class="w-full max-w-[52ch] rounded-md border border-line bg-panel p-4 shadow-2"
    >
      {#if failure}
        <ConnectSteps steps={steps(tab.progress, target, true)} />
        <div class="mt-4 border-t border-line pt-3">{@render outcome()}</div>
      {:else}
        {@render outcome()}
        <p class="mt-0.5 text-sm text-dim">Stopped before connecting to {tab.name}.</p>
      {/if}
      <div class="mt-4 flex flex-wrap justify-end gap-2">{@render actions()}</div>
    </div>
  </div>
{:else if tab.status.state === 'closed'}
  <!-- The last screen stays visible (dimmed) so the user can read what happened before the drop. -->
  <div class="pointer-events-none absolute inset-0 z-10 bg-screen/60"></div>
  <div
    role={failure ? 'alert' : 'status'}
    class="absolute inset-x-0 bottom-0 z-20 flex items-start gap-3 border-t border-line bg-panel px-4 py-3"
  >
    <div class="min-w-0 flex-1">{@render outcome()}</div>
    {@render actions()}
  </div>
{/if}
