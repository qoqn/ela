/// <reference types="bun" />
import { describe, expect, test } from 'bun:test';
import {
  DEFAULT_HOST_KEY as HOST,
  HostKey,
  appCommand,
  hostCommand,
  hostKeyLabel,
  isHostKeyCode,
  type KeyPress,
} from './shortcuts';

const press = (code: string, mods: Partial<KeyPress> = {}): KeyPress => ({
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});
const ctrl = { ctrlKey: true };
const chord = { ctrlKey: true, shiftKey: true };

describe('HostKey', () => {
  test('ordinary keys go to the remote side, down and up', () => {
    const key = new HostKey();
    expect(key.keydown('KeyA', false)).toEqual({ kind: 'forward' });
    expect(key.keyup('KeyA')).toEqual({ kind: 'forward' });
  });

  test('Ctrl+Shift goes to the remote side too', () => {
    const key = new HostKey();
    for (const code of ['ControlLeft', 'ShiftLeft', 'KeyW'])
      expect(key.keydown(code, false).kind).toBe('forward');
  });

  test('the host key itself never reaches the remote side', () => {
    const key = new HostKey();
    expect(key.keydown(HOST, false).kind).toBe('swallow');
    expect(key.keydown(HOST, true).kind).toBe('swallow');
  });

  test('tapping the host key alone is a tap', () => {
    const key = new HostKey();
    key.keydown(HOST, false);
    key.keydown(HOST, true);
    expect(key.keyup(HOST).kind).toBe('tap');
  });

  test('holding it and pressing a shortcut runs the command, without a tap', () => {
    const key = new HostKey();
    key.keydown(HOST, false);
    expect(key.keydown('KeyW', false)).toEqual({ kind: 'command', command: 'close_tab' });
    expect(key.keyup('KeyW').kind).toBe('swallow');
    expect(key.keyup(HOST).kind).toBe('swallow');
  });

  test('other keys while holding it go nowhere', () => {
    const key = new HostKey();
    key.keydown(HOST, false);
    expect(key.keydown('KeyQ', false).kind).toBe('swallow');
    expect(key.keyup('KeyQ').kind).toBe('swallow');
    expect(key.keyup(HOST).kind).toBe('swallow');
  });

  test('only tab cycling repeats while a key is held', () => {
    const key = new HostKey();
    key.keydown(HOST, false);
    expect(key.keydown('ArrowRight', true).kind).toBe('command');
    expect(key.keydown('KeyW', true).kind).toBe('swallow');
  });

  test('a key pressed before the host key still goes up on the remote side', () => {
    const key = new HostKey();
    key.keydown('KeyA', false);
    key.keydown(HOST, false);
    expect(key.keyup('KeyA').kind).toBe('forward');
  });

  test('a key-up without a forwarded key-down is dropped', () => {
    const key = new HostKey();
    key.keydown('KeyA', false);
    key.forgetForwarded();
    expect(key.keyup('KeyA').kind).toBe('swallow');
  });

  test('another key can be the host key', () => {
    const key = new HostKey();
    key.code = 'AltRight';
    expect(key.keydown('ControlRight', false).kind).toBe('forward');
    expect(key.keyup('ControlRight').kind).toBe('forward');
    expect(key.keydown('AltRight', false).kind).toBe('swallow');
    expect(key.keydown('KeyW', false)).toEqual({ kind: 'command', command: 'close_tab' });
    expect(key.keyup('AltRight').kind).toBe('swallow');
    key.keydown('AltRight', false);
    expect(key.keyup('AltRight').kind).toBe('tap');
  });

  test('losing the window ends a held host key', () => {
    const key = new HostKey();
    key.keydown(HOST, false);
    key.reset();
    expect(key.keydown('KeyW', false).kind).toBe('forward');
  });
});

describe('hostCommand', () => {
  test('covers the shortcut table', () => {
    expect(hostCommand('KeyP')).toBe('command_palette');
    expect(hostCommand('KeyT')).toBe('new_tab');
    expect(hostCommand('KeyO')).toBe('search_or_connect');
    expect(hostCommand('ArrowLeft')).toBe('previous_tab');
    expect(hostCommand('Digit1')).toBe('tab_1');
    expect(hostCommand('Digit9')).toBe('tab_9');
    expect(hostCommand('Digit0')).toBeNull();
    expect(hostCommand('KeyB')).toBe('toggle_sidebar');
    expect(hostCommand('KeyF')).toBe('fullscreen');
    expect(hostCommand('Slash')).toBe('shortcut_sheet');
  });
});

describe('appCommand', () => {
  test('plain Ctrl belongs to the shell in a terminal', () => {
    for (const code of ['KeyK', 'KeyT', 'KeyL', 'KeyW', 'KeyB', 'KeyC', 'KeyR']) {
      expect(appCommand(press(code, ctrl), 'terminal')).toBeNull();
    }
  });

  test('the same keys are shortcuts elsewhere', () => {
    expect(appCommand(press('KeyK', ctrl), 'elsewhere')).toBe('command_palette');
    expect(appCommand(press('KeyT', ctrl), 'elsewhere')).toBe('new_tab');
    expect(appCommand(press('KeyL', ctrl), 'elsewhere')).toBe('search_or_connect');
    expect(appCommand(press('KeyW', ctrl), 'elsewhere')).toBe('close_tab');
    expect(appCommand(press('KeyB', ctrl), 'elsewhere')).toBe('toggle_sidebar');
    expect(appCommand(press('Slash', ctrl), 'elsewhere')).toBe('shortcut_sheet');
  });

  test('app chords work in both places', () => {
    for (const where of ['terminal', 'elsewhere'] as const) {
      expect(appCommand(press('KeyP', chord), where)).toBe('command_palette');
      expect(appCommand(press('KeyT', chord), where)).toBe('new_tab');
      expect(appCommand(press('KeyW', chord), where)).toBe('close_tab');
      expect(appCommand(press('KeyO', chord), where)).toBe('search_or_connect');
      expect(appCommand(press('KeyB', chord), where)).toBe('toggle_sidebar');
      expect(appCommand(press('Slash', chord), where)).toBe('shortcut_sheet');
      expect(appCommand(press('PageDown', ctrl), where)).toBe('next_tab');
      expect(appCommand(press('PageUp', ctrl), where)).toBe('previous_tab');
      expect(appCommand(press('Digit3', { altKey: true }), where)).toBe('tab_3');
      expect(appCommand(press('F11'), where)).toBe('fullscreen');
    }
  });

  test('letters typed normally are not shortcuts', () => {
    expect(appCommand(press('KeyW'), 'elsewhere')).toBeNull();
    expect(appCommand(press('Digit3'), 'terminal')).toBeNull();
  });
});

describe('host key choices', () => {
  test('only known keys are accepted', () => {
    expect(isHostKeyCode('AltRight')).toBe(true);
    expect(isHostKeyCode('KeyA')).toBe(false);
    expect(isHostKeyCode(null)).toBe(false);
  });

  test('keys have names', () => {
    expect(hostKeyLabel('AltRight')).toBe('Right Alt');
  });
});

describe('settings and terminal text size', () => {
  const ctrl = { ctrlKey: true, shiftKey: false, altKey: false };
  const ctrlShift = { ctrlKey: true, shiftKey: true, altKey: false };
  const at = (code: string, chord: typeof ctrl) => ({ code, ...chord });

  test('plain Ctrl stays with the shell inside a terminal, Ctrl+Shift does not', () => {
    for (const code of ['Comma', 'Equal', 'Minus', 'Digit0']) {
      expect(appCommand(at(code, ctrl), 'terminal')).toBeNull();
    }
    expect(appCommand(at('Comma', ctrlShift), 'terminal')).toBe('settings');
    expect(appCommand(at('Equal', ctrlShift), 'terminal')).toBe('font_larger');
    expect(appCommand(at('Minus', ctrlShift), 'terminal')).toBe('font_smaller');
    expect(appCommand(at('Digit0', ctrlShift), 'terminal')).toBe('font_reset');
  });

  test('plain Ctrl works elsewhere', () => {
    expect(appCommand(at('Comma', ctrl), 'elsewhere')).toBe('settings');
    expect(appCommand(at('Equal', ctrl), 'elsewhere')).toBe('font_larger');
  });

  test('the host key reaches Settings from a remote desktop', () => {
    expect(hostCommand('Comma')).toBe('settings');
  });
});
