// Who owns which key (design: Keyboard ownership). No state and no
// DOM here, so the rules can be tested on their own.
import type { HostKeyCode } from '../ipc';

type TabDigit = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;

export type AppCommand =
  | 'command_palette'
  | 'new_tab'
  | 'search_or_connect'
  | 'close_tab'
  | 'next_tab'
  | 'previous_tab'
  | 'toggle_sidebar'
  | 'fullscreen'
  | 'shortcut_sheet'
  | 'settings'
  | 'font_larger'
  | 'font_smaller'
  | 'font_reset'
  | `tab_${TabDigit}`;

// Several choices, because keyboards differ: many laptops have no Right Ctrl. The codes come
// from Rust, which keeps the choice in settings.toml.
const HOST_KEY_LABELS: Record<HostKeyCode, string> = {
  ControlRight: 'Right Ctrl',
  AltRight: 'Right Alt',
  ShiftRight: 'Right Shift',
  ContextMenu: 'Menu',
  ScrollLock: 'Scroll Lock',
  Pause: 'Pause',
};

export const HOST_KEYS = (Object.keys(HOST_KEY_LABELS) as HostKeyCode[]).map((code) => ({
  code,
  label: HOST_KEY_LABELS[code],
}));

export type { HostKeyCode };
export const DEFAULT_HOST_KEY: HostKeyCode = 'ControlRight';

export function hostKeyLabel(code: string): string {
  return HOST_KEYS.find((key) => key.code === code)?.label ?? code;
}

export function isHostKeyCode(code: string | null): code is HostKeyCode {
  return HOST_KEYS.some((key) => key.code === code);
}

export const SHORTCUTS: {
  action: string;
  command?: AppCommand;
  terminal: string;
  desktop: string | null;
  elsewhere: string | null;
}[] = [
  { action: 'Command palette', terminal: 'Ctrl+Shift+P', desktop: 'Host+P', elsewhere: 'Ctrl+K' },
  {
    action: 'New tab',
    command: 'new_tab',
    terminal: 'Ctrl+Shift+T',
    desktop: 'Host+T',
    elsewhere: 'Ctrl+T',
  },
  {
    action: 'Search or connect',
    command: 'search_or_connect',
    terminal: 'Ctrl+Shift+O',
    desktop: 'Host+O',
    elsewhere: 'Ctrl+L',
  },
  {
    action: 'Close tab',
    command: 'close_tab',
    terminal: 'Ctrl+Shift+W',
    desktop: 'Host+W',
    elsewhere: 'Ctrl+W',
  },
  {
    action: 'Next tab',
    command: 'next_tab',
    terminal: 'Ctrl+PageDown',
    desktop: 'Host+→',
    elsewhere: 'Ctrl+PageDown',
  },
  {
    action: 'Previous tab',
    command: 'previous_tab',
    terminal: 'Ctrl+PageUp',
    desktop: 'Host+←',
    elsewhere: 'Ctrl+PageUp',
  },
  { action: 'Go to tab 1–9', terminal: 'Alt+1…9', desktop: 'Host+1…9', elsewhere: 'Alt+1…9' },
  {
    action: 'Toggle sidebar',
    command: 'toggle_sidebar',
    terminal: 'Ctrl+Shift+B',
    desktop: 'Host+B',
    elsewhere: 'Ctrl+B',
  },
  {
    action: 'Copy',
    terminal: 'Ctrl+Shift+C',
    desktop: 'As on the remote side',
    elsewhere: 'Ctrl+C',
  },
  {
    action: 'Paste',
    terminal: 'Ctrl+Shift+V',
    desktop: 'As on the remote side',
    elsewhere: 'Ctrl+V',
  },
  {
    action: 'Find in the terminal',
    terminal: 'Ctrl+Shift+F',
    desktop: null,
    elsewhere: null,
  },
  {
    action: 'Fullscreen',
    command: 'fullscreen',
    terminal: 'F11',
    desktop: 'Host+F',
    elsewhere: 'F11',
  },
  {
    action: 'Shortcut sheet',
    command: 'shortcut_sheet',
    terminal: 'Ctrl+Shift+/',
    desktop: 'Host+/',
    elsewhere: 'Ctrl+/',
  },
  {
    action: 'Settings',
    command: 'settings',
    terminal: 'Ctrl+Shift+,',
    desktop: 'Host+,',
    elsewhere: 'Ctrl+,',
  },
  {
    action: 'Larger terminal text',
    command: 'font_larger',
    terminal: 'Ctrl+Shift+=',
    desktop: null,
    elsewhere: 'Ctrl+=',
  },
  {
    action: 'Smaller terminal text',
    command: 'font_smaller',
    terminal: 'Ctrl+Shift+-',
    desktop: null,
    elsewhere: 'Ctrl+-',
  },
  {
    action: 'Default terminal text size',
    command: 'font_reset',
    terminal: 'Ctrl+Shift+0',
    desktop: null,
    elsewhere: 'Ctrl+0',
  },
];

export type KeyPress = Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'shiftKey' | 'altKey'>;

function tabCommand(code: string): AppCommand | null {
  const digit = /^Digit([1-9])$/.exec(code)?.[1];
  return digit ? (`tab_${digit}` as AppCommand) : null;
}

export function hostCommand(code: string): AppCommand | null {
  const commands: Record<string, AppCommand> = {
    KeyP: 'command_palette',
    KeyT: 'new_tab',
    KeyO: 'search_or_connect',
    KeyW: 'close_tab',
    ArrowRight: 'next_tab',
    ArrowLeft: 'previous_tab',
    KeyB: 'toggle_sidebar',
    KeyF: 'fullscreen',
    Slash: 'shortcut_sheet',
    Comma: 'settings',
  };
  return commands[code] ?? tabCommand(code);
}

const FONT_AND_SETTINGS_KEYS: Record<string, AppCommand> = {
  Comma: 'settings',
  Equal: 'font_larger',
  Minus: 'font_smaller',
  Digit0: 'font_reset',
};

export function appCommand(e: KeyPress, where: 'terminal' | 'elsewhere'): AppCommand | null {
  if (e.code === 'F11') return 'fullscreen';
  if (e.ctrlKey && e.code === 'PageDown') return 'next_tab';
  if (e.ctrlKey && e.code === 'PageUp') return 'previous_tab';
  if (e.altKey && !e.ctrlKey && !e.shiftKey) return tabCommand(e.code);
  if (e.ctrlKey && e.shiftKey) {
    const chords: Record<string, AppCommand> = {
      KeyP: 'command_palette',
      KeyT: 'new_tab',
      KeyO: 'search_or_connect',
      KeyW: 'close_tab',
      KeyB: 'toggle_sidebar',
      Slash: 'shortcut_sheet',
      ...FONT_AND_SETTINGS_KEYS,
    };
    return chords[e.code] ?? null;
  }
  // Plain Ctrl combinations belong to the shell inside a terminal.
  if (e.ctrlKey && where === 'elsewhere') {
    const plain: Record<string, AppCommand> = {
      KeyK: 'command_palette',
      KeyT: 'new_tab',
      KeyL: 'search_or_connect',
      KeyW: 'close_tab',
      KeyB: 'toggle_sidebar',
      Slash: 'shortcut_sheet',
      ...FONT_AND_SETTINGS_KEYS,
    };
    return plain[e.code] ?? null;
  }
  return null;
}

export type KeyDecision =
  | { kind: 'forward' }
  | { kind: 'swallow' }
  | { kind: 'command'; command: AppCommand }
  | { kind: 'tap' };

/**
 * The host key of framebuffer Sessions. Holding it across Tabs must keep working
 * (`Host+→`, `→`), so there is one for the whole app. A key only goes up on the remote
 * side if it also went down there.
 */
export class HostKey {
  code: HostKeyCode = DEFAULT_HOST_KEY;
  down = false;
  #usedInChord = false;
  #forwarded = new Set<string>();

  keydown(code: string, repeat: boolean): KeyDecision {
    if (code === this.code) {
      if (!repeat) {
        this.down = true;
        this.#usedInChord = false;
      }
      return { kind: 'swallow' };
    }
    if (this.down) {
      this.#usedInChord = true;
      const command = hostCommand(code);
      // Holding Host+W must not close every Tab, while Host+→ may repeat.
      if (!command || (repeat && command !== 'next_tab' && command !== 'previous_tab'))
        return { kind: 'swallow' };
      return { kind: 'command', command };
    }
    this.#forwarded.add(code);
    return { kind: 'forward' };
  }

  keyup(code: string): KeyDecision {
    if (code === this.code) {
      const tapped = this.down && !this.#usedInChord;
      this.down = false;
      return { kind: tapped ? 'tap' : 'swallow' };
    }
    return { kind: this.#forwarded.delete(code) ? 'forward' : 'swallow' };
  }

  forgetForwarded() {
    this.#forwarded.clear();
  }

  /** The window lost focus, so the host key's own key-up may never arrive. */
  reset() {
    this.down = false;
    this.#usedInChord = false;
    this.#forwarded.clear();
  }
}

export const hostKey = new HostKey();
