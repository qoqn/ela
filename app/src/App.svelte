<script lang="ts">
  import { onMount } from 'svelte';
  import DevToolsDialog from './lib/features/devtools/DevToolsDialog.svelte';
  import { reconnect } from './lib/features/sessions/reconnect.svelte';
  import FilesPanel from './lib/features/files/FilesPanel.svelte';
  import SessionPane from './lib/features/sessions/SessionPane.svelte';
  import SettingsPage from './lib/features/settings/SettingsPage.svelte';
  import { terminalFont } from './lib/features/settings/terminalFont.svelte';
  import Sidebar from './lib/features/sidebar/Sidebar.svelte';
  import ConnectionEditor from './lib/features/tree/ConnectionEditor.svelte';
  import FolderEditor from './lib/features/tree/FolderEditor.svelte';
  import ImportNotes from './lib/features/tree/ImportNotes.svelte';
  import ImportSshHosts from './lib/features/tree/ImportSshHosts.svelte';
  import MoveDialog from './lib/features/tree/MoveDialog.svelte';
  import { saved } from './lib/features/tree/saved.svelte';
  import TrashDialog from './lib/features/tree/TrashDialog.svelte';
  import KnownHostsPanel from './lib/features/knownhosts/KnownHostsPanel.svelte';
  import { vault } from './lib/features/vault/vault.svelte';
  import VaultDialog from './lib/features/vault/VaultDialog.svelte';
  import VaultPanel from './lib/features/vault/VaultPanel.svelte';
  import Welcome from './lib/features/welcome/Welcome.svelte';
  import { workspace } from './lib/sessions.svelte';
  import ActivityRail from './lib/shell/ActivityRail.svelte';
  import StatusBar from './lib/shell/StatusBar.svelte';
  import { runCommand, shell } from './lib/shell/shell.svelte';
  import CommandPalette from './lib/shell/CommandPalette.svelte';
  import ShortcutSheet from './lib/shell/ShortcutSheet.svelte';
  import { appCommand, hostKey } from './lib/shell/shortcuts';
  import TabStrip from './lib/shell/TabStrip.svelte';
  import { ResizeHandle, Toast } from './lib/ui';
  import { PANEL_DEFAULT, PANEL_MAX, PANEL_MIN } from './lib/shell/panelWidth';

  let loadError = $state('');
  onMount(() => {
    workspace.load().catch((e) => (loadError = String(e)));
    void shell.loadHostKey();
    void terminalFont.load();
  });

  // Framebuffer Sessions handle their own keys and don't let them reach this far.
  function onKeydown(e: KeyboardEvent) {
    const inTerminal = e.target instanceof Element && e.target.closest('.xterm') !== null;
    const command = appCommand(e, inTerminal ? 'terminal' : 'elsewhere');
    if (command) {
      e.preventDefault();
      runCommand(command);
    }
  }

  // The webview's own menu offers Reload and Back, which would drop every Session. Text
  // fields and a selection keep it for copy and paste; anywhere else Ela shows its own Menu or
  // nothing.
  function onContextMenu(e: MouseEvent) {
    if (e.defaultPrevented) return;
    const target = e.target instanceof Element ? e.target : null;
    if (target?.closest('input, textarea, [contenteditable="true"]')) return;
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed && target?.closest('.selectable')) return;
    e.preventDefault();
  }

  // The host key can be let go anywhere, for example after Host+/ moved focus to a dialog.
  function onKeyup(e: KeyboardEvent) {
    if (e.code === hostKey.code) hostKey.keyup(e.code);
  }
</script>

<svelte:window
  onkeydown={onKeydown}
  onkeyup={onKeyup}
  onblur={() => hostKey.reset()}
  oncontextmenu={onContextMenu}
/>

<div class="relative flex h-full flex-col">
  <div class="flex min-h-0 flex-1">
    <ActivityRail />
    <div
      class="relative shrink-0"
      class:hidden={!shell.sidebarOpen}
      style:width="{shell.panelWidth}px"
    >
      <div class="h-full" class:hidden={shell.section !== 'connections'}><Sidebar /></div>
      {#if shell.sidebarOpen && shell.section === 'files'}<FilesPanel />{/if}
      {#if shell.sidebarOpen && shell.section === 'vault'}<VaultPanel />{/if}
      {#if shell.sidebarOpen && shell.section === 'knownhosts'}<KnownHostsPanel />{/if}
      <ResizeHandle
        label="Sidebar width"
        value={shell.panelWidth}
        min={PANEL_MIN}
        max={PANEL_MAX}
        fallback={PANEL_DEFAULT[shell.section]}
        onchange={(width) => shell.setPanelWidth(width)}
      />
    </div>
    <main class="relative flex min-w-0 flex-1 flex-col">
      <!-- Inert under Settings, so no key or Tab stop can reach a Session unseen. -->
      <div class="flex min-h-0 flex-1 flex-col" inert={shell.settingsOpen}>
        {#if workspace.tabs.length}
          <TabStrip />
        {/if}
        <div class="relative min-h-0 flex-1">
          {#each workspace.tabs as tab (tab.key)}
            <div class="absolute inset-0" class:invisible={tab.key !== workspace.activeKey}>
              <SessionPane {tab} active={tab.key === workspace.activeKey && !shell.settingsOpen} />
            </div>
          {:else}
            <Welcome {loadError} />
          {/each}
        </div>
      </div>
      {#if shell.settingsOpen}<SettingsPage />{/if}
    </main>
  </div>
  <StatusBar />
  {#if shell.sheetOpen}<ShortcutSheet />{/if}
  {#if shell.palette}<CommandPalette />{/if}
  {#if saved.editing?.kind === 'connection'}
    {#key saved.editing.draft.id}<ConnectionEditor
        draft={saved.editing.draft}
        isNew={saved.editing.isNew}
      />{/key}
  {:else if saved.editing?.kind === 'folder'}
    {#key saved.editing.draft.id}<FolderEditor
        draft={saved.editing.draft}
        isNew={saved.editing.isNew}
      />{/key}
  {/if}
  {#if saved.askingImport}<ImportSshHosts />{/if}
  {#if saved.imported}<ImportNotes result={saved.imported} />{/if}
  {#if saved.moving}<MoveDialog moving={saved.moving} />{/if}
  {#if saved.trashOpen}<TrashDialog />{/if}
  {#if import.meta.env.DEV && shell.devToolsOpen}<DevToolsDialog />{/if}
  {#if vault.asking}
    <VaultDialog reason={vault.asking.reason} creating={vault.info?.state === 'absent'} />
  {/if}
  {#if saved.toast}
    <Toast
      message={saved.toast.message}
      detail={saved.toast.detail}
      error={saved.toast.error}
      onundo={saved.toast.undo}
      ondismiss={() => saved.dismissToast()}
    />
  {/if}
</div>
