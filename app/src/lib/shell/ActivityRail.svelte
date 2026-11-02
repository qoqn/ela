<script lang="ts">
  import {
    Fingerprint,
    FlaskConical,
    FolderOpen,
    KeyRound,
    Server,
    Settings,
  } from '@lucide/svelte';
  import { IconButton } from '../ui';
  import { runCommand, shell } from './shell.svelte';

  // Tunnels and Snippets join once they do something (design: Regions).
  const items = [
    { section: 'connections', label: 'Connections', icon: Server },
    { section: 'files', label: 'Files', icon: FolderOpen },
    { section: 'vault', label: 'Vault', icon: KeyRound },
    { section: 'knownhosts', label: 'Known Hosts', icon: Fingerprint },
  ] as const;
</script>

<nav
  class="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-line bg-panel py-2"
  aria-label="Sections"
>
  {#each items as item (item.section)}
    <IconButton
      label={item.label}
      active={item.section === shell.section && !shell.settingsOpen}
      onclick={() => {
        shell.section = item.section;
        shell.sidebarOpen = true;
        shell.settingsOpen = false;
      }}
    >
      <item.icon size={18} strokeWidth={1.75} />
    </IconButton>
  {/each}
  <div class="mt-auto flex flex-col gap-1">
    {#if import.meta.env.DEV}
      <IconButton
        label="Development tools"
        active={shell.devToolsOpen}
        onclick={() => (shell.devToolsOpen = true)}
      >
        <FlaskConical size={18} strokeWidth={1.75} />
      </IconButton>
    {/if}
    <IconButton label="Settings" active={shell.settingsOpen} onclick={() => runCommand('settings')}>
      <Settings size={18} strokeWidth={1.75} />
    </IconButton>
  </div>
</nav>
