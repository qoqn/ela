import { getCurrentWindow } from '@tauri-apps/api/window';
import { commands } from '../bindings';
import { DEFAULT_SIZE } from '../features/settings/terminalFont';
import { terminalFont } from '../features/settings/terminalFont.svelte';
import type { SettingsSection } from '../features/settings/sections';
import { workspace } from '../sessions.svelte';
import { LEGACY_HOST_KEY, loadHostKey } from './hostKeySetting';
import {
  clampPanelWidth,
  PANEL_DEFAULT,
  readPanelWidths,
  type PanelWidths,
  type Section,
} from './panelWidth';
import {
  DEFAULT_HOST_KEY,
  hostKey,
  hostKeyLabel,
  isHostKeyCode,
  type AppCommand,
  type HostKeyCode,
} from './shortcuts';

const PANEL_WIDTHS_SETTING = 'ela.panelWidths';

class Shell {
  sidebarOpen = $state(true);
  section = $state<Section>('connections');
  #panelWidths = $state<PanelWidths>(readPanelWidths(localStorage.getItem(PANEL_WIDTHS_SETTING)));
  panelWidth = $derived(this.#panelWidths[this.section] ?? PANEL_DEFAULT[this.section]);
  sheetOpen = $state(false);
  settingsOpen = $state(false);
  settingsTarget = $state<SettingsSection | null>(null);
  devToolsOpen = $state(false);
  hostKeyCode = $state<HostKeyCode>(DEFAULT_HOST_KEY);
  hostKeyName = $derived(hostKeyLabel(this.hostKeyCode));
  hostKeyError = $state('');
  hostKeyProblem = $derived(
    this.hostKeyError &&
      `${this.hostKeyName} works until Ela closes, but it couldn't be saved: ${this.hostKeyError}`,
  );
  #hostKeyChosen = false;
  palette = $state<'commands' | 'connect' | null>(null);
  focusSearch: () => void = () => {};
  connectQuickly: (address: string) => void | Promise<void> = () => {};

  constructor() {
    hostKey.code = this.hostKeyCode;
    workspace.onSelected = () => (this.settingsOpen = false);
  }

  openSettings(section: SettingsSection | null = null) {
    this.settingsTarget = section;
    this.settingsOpen = true;
  }

  setPanelWidth(width: number) {
    this.#panelWidths[this.section] = clampPanelWidth(width);
    localStorage.setItem(PANEL_WIDTHS_SETTING, JSON.stringify(this.#panelWidths));
  }

  async loadHostKey() {
    let code: HostKeyCode | null;
    try {
      code = await loadHostKey(localStorage, commands);
    } catch (e) {
      console.warn('could not read the host key from settings.toml', e);
      const legacy = localStorage.getItem(LEGACY_HOST_KEY);
      code = isHostKeyCode(legacy) ? legacy : null;
    }
    // A key chosen while this was loading is newer than what was read.
    if (code && !this.#hostKeyChosen) this.#useHostKey(code);
  }

  async setHostKey(code: HostKeyCode) {
    this.#hostKeyChosen = true;
    this.#useHostKey(code);
    try {
      await commands.setHostKey(code);
      this.hostKeyError = '';
    } catch (e) {
      this.hostKeyError = String(e);
    }
  }

  #useHostKey(code: HostKeyCode) {
    this.hostKeyCode = code;
    hostKey.code = code;
    hostKey.reset();
  }

  /** Through the search field, so a mistyped address shows its error there. */
  connectTo(address: string) {
    this.sidebarOpen = true;
    this.section = 'connections';
    requestAnimationFrame(() => this.connectQuickly(address));
  }
}

export const shell = new Shell();

async function toggleFullscreen() {
  const window = getCurrentWindow();
  await window.setFullscreen(!(await window.isFullscreen()));
}

export function runCommand(command: AppCommand) {
  switch (command) {
    case 'command_palette':
      shell.palette = shell.palette === 'commands' ? null : 'commands';
      break;
    case 'new_tab':
      shell.palette = shell.palette === 'connect' ? null : 'connect';
      break;
    case 'search_or_connect':
      shell.sidebarOpen = true;
      shell.section = 'connections';
      // The field can't take focus until the sidebar is visible again.
      requestAnimationFrame(() => shell.focusSearch());
      break;
    case 'close_tab':
      if (shell.settingsOpen) shell.settingsOpen = false;
      else if (workspace.activeKey) workspace.closeTab(workspace.activeKey);
      break;
    case 'next_tab':
      workspace.cycle(1);
      break;
    case 'previous_tab':
      workspace.cycle(-1);
      break;
    case 'toggle_sidebar':
      shell.sidebarOpen = !shell.sidebarOpen;
      break;
    case 'fullscreen':
      toggleFullscreen().catch((e) => console.warn('could not change fullscreen', e));
      break;
    case 'shortcut_sheet':
      shell.sheetOpen = !shell.sheetOpen;
      break;
    case 'settings':
      if (shell.settingsOpen) shell.settingsOpen = false;
      else shell.openSettings();
      break;
    case 'font_larger':
      terminalFont.setSize(terminalFont.size + 1);
      break;
    case 'font_smaller':
      terminalFont.setSize(terminalFont.size - 1);
      break;
    case 'font_reset':
      terminalFont.setSize(DEFAULT_SIZE);
      break;
    default: {
      const tab = workspace.tabs[Number(command.slice('tab_'.length)) - 1];
      if (tab) workspace.select(tab.key);
    }
  }
}
