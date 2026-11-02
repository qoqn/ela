<script lang="ts">
  import { Button, Dialog } from '../ui';
  import HostKeySelect from './HostKeySelect.svelte';
  import { shell } from './shell.svelte';
  import { SHORTCUTS } from './shortcuts';

  function close() {
    shell.sheetOpen = false;
  }

  // "As on the remote side" is a sentence, not keys to press.
  const isKeys = (text: string) => !text.includes(' ');
</script>

{#snippet keys(text: string | null)}
  {#if text === null}
    <span class="text-dim" aria-label="None">—</span>
  {:else if isKeys(text)}
    <span class="font-mono text-xs">{text}</span>
  {:else}
    <span class="text-dim">{text}</span>
  {/if}
{/snippet}

<Dialog title="Shortcuts" onclose={close} width="80ch">
  <label class="flex items-center gap-2 text-sm">
    Host key
    <HostKeySelect data-autofocus />
  </label>
  {#if shell.hostKeyProblem}
    <p class="mt-1 text-sm text-danger-text selectable" role="alert">{shell.hostKeyProblem}</p>
  {/if}
  <p class="mt-1 text-sm text-dim">
    In a remote desktop, tap it alone to release the keyboard, or hold it with a key below. It never
    reaches the remote side.
  </p>
  <table class="mt-3 w-full text-sm">
    <thead class="text-left text-dim">
      <tr>
        <th class="py-1 font-normal">Action</th>
        <th class="font-normal">Terminal</th>
        <th class="font-normal">Remote desktop</th>
        <th class="font-normal">Elsewhere</th>
      </tr>
    </thead>
    <tbody>
      {#each SHORTCUTS as { action, terminal, desktop, elsewhere } (action)}
        <tr class="border-t border-line">
          <td class="py-1">{action}</td>
          <td>{@render keys(terminal)}</td>
          <td>{@render keys(desktop)}</td>
          <td>{@render keys(elsewhere)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#snippet footer()}
    <Button onclick={close}>Close</Button>
  {/snippet}
</Dialog>
