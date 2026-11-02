<script lang="ts">
  import { Activity } from '@lucide/svelte';
  import { IconButton, Kbd, Led } from '../ui';
  import { workspace } from '../sessions.svelte';
  import { stateLabel } from '../features/sessions/copy';
  import { shell } from './shell.svelte';

  const tab = $derived(workspace.active);
  const hostKeyShown = $derived(
    tab?.renderer === 'framebuffer' && tab.status.state === 'connected',
  );
</script>

<footer
  class="flex h-6 shrink-0 items-center gap-4 border-t border-line bg-panel px-3 text-xs text-dim"
>
  {#if tab}
    <span class="flex items-center gap-1.5 text-text">
      <Led state={tab.linkState} />
      {stateLabel(tab.status)}
    </span>
    <span>{tab.protocol.toUpperCase()}</span>
    <span class="font-mono selectable">{tab.address}</span>
    {#if tab.fingerprint}<span class="truncate font-mono selectable" title="Server identity"
        >{tab.fingerprint}</span
      >{/if}
  {:else}
    <span>No session selected</span>
  {/if}
  <div class="ml-auto flex shrink-0 items-center gap-4">
    {#if hostKeyShown}
      <!-- Reachable by mouse, for a keyboard without this key. Pressing it keeps the keyboard
           with the Session until the sheet opens and takes focus. -->
      <button
        type="button"
        title="Shortcuts and host key"
        class="flex items-center gap-1.5 hover:text-text"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => (shell.sheetOpen = true)}
      >
        Host key <Kbd keys={shell.hostKeyName} />
      </button>
    {/if}
    {#if tab}
      <IconButton
        size="sm"
        label={tab.statsOpen ? 'Hide throughput' : 'Show throughput'}
        active={tab.statsOpen}
        disabled={tab.status.state !== 'connected'}
        onmousedown={(e) => e.preventDefault()}
        onclick={() => (tab.statsOpen = !tab.statsOpen)}
      >
        <Activity size={14} />
      </IconButton>
    {/if}
    <span class="tabular"
      >{workspace.tabs.length} {workspace.tabs.length === 1 ? 'session' : 'sessions'}</span
    >
  </div>
</footer>
