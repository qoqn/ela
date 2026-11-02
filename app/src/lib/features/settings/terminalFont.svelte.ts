import { commands } from '../../bindings';
import { clampSize, DEFAULT_SIZE, fontStack } from './terminalFont';

class TerminalFontSetting {
  family = $state<string | null>(null);
  size = $state(DEFAULT_SIZE);
  error = $state('');
  system = $state<string[]>([]);
  systemLoaded = $state(false);
  stack = $derived(fontStack(this.family));

  async load() {
    try {
      const { family, size } = await commands.terminalFont();
      this.family = family;
      this.size = clampSize(size ?? DEFAULT_SIZE);
    } catch (e) {
      console.warn('could not read the terminal font from settings.toml', e);
    }
  }

  async loadSystemFonts() {
    if (this.systemLoaded) return;
    try {
      this.system = await commands.monospaceFonts();
    } catch (e) {
      console.warn('could not list the system fonts', e);
    }
    this.systemLoaded = true;
  }

  setFamily(family: string | null) {
    this.family = family;
    void this.#save();
  }

  setSize(size: number) {
    this.size = clampSize(size);
    void this.#save();
  }

  async #save() {
    try {
      await commands.setTerminalFont({
        family: this.family,
        size: this.size === DEFAULT_SIZE ? null : this.size,
      });
      this.error = '';
    } catch (e) {
      this.error = String(e);
    }
  }
}

export const terminalFont = new TerminalFontSetting();
