<script lang="ts">
  import { terminalFont } from '../settings/terminalFont.svelte';
  import { onMount, tick } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { SearchAddon, type ISearchOptions } from '@xterm/addon-search';
  import { WebglAddon } from '@xterm/addon-webgl';
  import '@xterm/xterm/css/xterm.css';
  import { copyText, pasteText } from '../../clipboard';
  import { MSG_TERMINAL } from '../../ipc';
  import { workspace, type SessionTab } from '../../sessions.svelte';
  import { shell } from '../../shell/shell.svelte';
  import { appCommand } from '../../shell/shortcuts';
  import { Menu, type MenuItem } from '../../ui';
  import { LATENCY_OSC } from '../devtools/latencyStats';
  import { fileBrowserOf } from '../files/files.svelte';
  import { findPatternError } from './find';
  import { folderFromOsc7 } from './osc7';
  import TerminalFindBar from './TerminalFindBar.svelte';

  let { tab, active }: { tab: SessionTab; active: boolean } = $props();

  let host: HTMLDivElement;
  let term: Terminal | undefined;
  let fit: FitAddon | undefined;
  let search: SearchAddon | undefined;
  let findField = $state<HTMLInputElement>();
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  // Only shells that send OSC 7 say where they are; the rest leave Files where it was.
  let folder: string | null = null;

  function refit() {
    if (!term || !fit || host.clientWidth === 0) return;
    fit.fit();
  }

  function copySelection() {
    if (term?.hasSelection()) void copyText(term.getSelection());
  }

  function paste() {
    void pasteText().then((text) => text && term?.paste(text));
  }

  const FIND_LIMIT = 1000;
  // VS Code 2026 Dark's find colours over the screen background, because decorations only
  // take #RRGGBB. Its two match colours are nearly the same, so the selected one also gets
  // the signal border (design: Terminal palette).
  const FIND_DECORATIONS = {
    matchBackground: '#1d3d4b',
    matchOverviewRuler: '#3a94bc',
    activeMatchBackground: '#1e4252',
    activeMatchBorder: '#3994bc',
    activeMatchColorOverviewRuler: '#3994bc',
  };

  function findOptions(incremental: boolean): ISearchOptions {
    const { matchCase, regex } = tab.find;
    return { caseSensitive: matchCase, regex, incremental, decorations: FIND_DECORATIONS };
  }

  function step(forward: boolean, incremental = false) {
    const { query, regex } = tab.find;
    if (!search) return;
    if (!query || findPatternError(query, regex)) {
      search.clearDecorations();
      tab.find.results = { index: -1, count: 0 };
      return;
    }
    if (forward) search.findNext(query, findOptions(incremental));
    else search.findPrevious(query, findOptions(incremental));
  }

  async function openFind() {
    const selected = term?.getSelection() ?? '';
    if (selected && !selected.includes('\n')) tab.find.query = selected;
    tab.find.open = true;
    await tick();
    findField?.focus();
    findField?.select();
  }

  $effect(() => {
    tab.find.show = openFind;
  });

  function closeFind() {
    tab.find.open = false;
    search?.clearDecorations();
    term?.focus();
  }

  $effect(() => {
    if (!tab.find.open) return;
    void [tab.find.query, tab.find.matchCase, tab.find.regex];
    step(true, true);
  });

  function openFilesHere() {
    shell.sidebarOpen = true;
    shell.section = 'files';
    if (folder && tab.status.state === 'connected') void fileBrowserOf(tab).open(folder);
  }

  function menuItems(): MenuItem[] {
    const items: MenuItem[] = [
      { label: 'Copy', keys: 'Ctrl+Shift+C', disabled: !term?.hasSelection(), run: copySelection },
      { label: 'Paste', keys: 'Ctrl+Shift+V', run: paste },
      { label: 'Find', keys: 'Ctrl+Shift+F', run: () => void openFind() },
    ];
    if (workspace.protocol(tab.protocol)?.capabilities.file_transfer) {
      items.push({ label: 'Open Files here', run: openFilesHere });
    }
    return items;
  }

  function withoutShift(e: MouseEvent) {
    const { bubbles, cancelable, view, detail, screenX, screenY, clientX, clientY } = e;
    const { ctrlKey, altKey, metaKey, button, buttons } = e;
    return new MouseEvent(e.type, {
      ...{ bubbles, cancelable, view, detail, screenX, screenY, clientX, clientY },
      ...{ ctrlKey, altKey, metaKey, button, buttons, shiftKey: false },
    });
  }

  function handOn(e: MouseEvent) {
    e.stopPropagation();
    e.target?.dispatchEvent(withoutShift(e));
  }

  // xterm treats Shift as "select, don't report", so a Shift+right-click is handed on as a
  // plain one; a plain right-click never reaches xterm and opens the menu instead.
  function onMouseDown(e: MouseEvent) {
    if (e.button !== 2 || !e.isTrusted) return;
    if (!e.shiftKey) return e.stopPropagation();
    handOn(e);
    const release = (up: MouseEvent) => {
      if (up.button !== 2 || !up.isTrusted) return;
      window.removeEventListener('mouseup', release, true);
      handOn(up);
    };
    window.addEventListener('mouseup', release, true);
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!e.shiftKey) menu = { x: e.clientX, y: e.clientY, items: menuItems() };
  }

  onMount(() => {
    const t = new Terminal({
      fontFamily: terminalFont.stack,
      fontSize: terminalFont.size,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 10_000,
      allowProposedApi: true,
      // VS Code's default terminal colours on the screen background (design: Terminal palette).
      theme: {
        background: '#121314',
        foreground: '#cccccc',
        cursor: '#bfbfbf',
        cursorAccent: '#121314',
        selectionBackground: '#3994bc55',
        black: '#000000',
        red: '#cd3131',
        green: '#0dbc79',
        yellow: '#e5e510',
        blue: '#2472c8',
        magenta: '#bc3fbc',
        cyan: '#11a8cd',
        white: '#e5e5e5',
        brightBlack: '#666666',
        brightRed: '#f14c4c',
        brightGreen: '#23d18b',
        brightYellow: '#f5f543',
        brightBlue: '#3b8eea',
        brightMagenta: '#d670d6',
        brightCyan: '#29b8db',
        brightWhite: '#e5e5e5',
      },
    });
    const f = new FitAddon();
    t.loadAddon(f);
    const s = new SearchAddon({ highlightLimit: FIND_LIMIT });
    t.loadAddon(s);
    t.open(host);
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl.dispose();
        tab.drawBackend = 'dom (webgl lost)';
      });
      t.loadAddon(webgl);
      tab.drawBackend = 'webgl';
    } catch {
      tab.drawBackend = 'dom';
    }
    // App chords (Ctrl+Shift+…) and tab switching belong to Ela, not the remote shell
    // (design: Keyboard ownership).
    t.attachCustomKeyEventHandler((e) => {
      if (e.ctrlKey && e.shiftKey && (e.code === 'KeyC' || e.code === 'KeyV')) {
        // Stops the webview's own paste, which would paste a second time.
        e.preventDefault();
        if (e.type === 'keydown' && e.code === 'KeyC') copySelection();
        if (e.type === 'keydown' && e.code === 'KeyV') paste();
        return false;
      }
      if (e.ctrlKey && e.shiftKey && e.code === 'KeyF') {
        e.preventDefault();
        if (e.type === 'keydown') void openFind();
        return false;
      }
      if (e.ctrlKey && e.shiftKey && /^Key[A-Z]$/.test(e.code)) return false;
      return appCommand(e, 'terminal') === null;
    });
    t.parser.registerOscHandler(7, (data) => {
      folder = folderFromOsc7(data);
      return true;
    });
    term = t;
    fit = f;
    search = s;
    refit();

    void tab.start({ width: t.cols, height: t.rows }, (msg) => {
      const bytes = new Uint8Array(msg);
      if (bytes[0] !== MSG_TERMINAL) return tab.ack();
      tab.terminalProbe?.received(bytes);
      // Ack once xterm has parsed the chunk: that is the backpressure signal.
      t.write(bytes.subarray(1), () => {
        tab.ack();
        tab.terminalProbe?.written();
      });
    });

    const subs = [
      t.onData((data) => tab.input({ type: 'text', data })),
      t.onResize(({ cols, rows }) => tab.input({ type: 'resize', width: cols, height: rows })),
      s.onDidChangeResults(({ resultIndex, resultCount }) => {
        tab.find.results = { index: resultIndex, count: resultCount };
      }),
    ];
    if (import.meta.env.DEV) {
      tab.drive = { type: (data) => t.input(data, true) };
      t.parser.registerOscHandler(LATENCY_OSC, (data) => {
        tab.terminalProbe?.echoParsed(data);
        return true;
      });
      subs.push(t.onRender(() => tab.terminalProbe?.rendered()));
    }
    const ro = new ResizeObserver(() => refit());
    ro.observe(host);

    return () => {
      ro.disconnect();
      subs.forEach((s) => s.dispose());
      t.dispose();
    };
  });

  $effect(() => {
    if (!term) return;
    term.options.fontFamily = terminalFont.stack;
    term.options.fontSize = terminalFont.size;
    refit();
  });

  // A derived flag, so other status changes don't pull focus away from where the user is.
  const awaitingUser = $derived(tab.linkState === 'waiting');

  // Focus comes back here whenever this Tab shows, and when a panel like the first-connection
  // one, which holds focus meanwhile, gives way to the connected Session.
  $effect(() => {
    if (active && !awaitingUser) {
      requestAnimationFrame(() => {
        refit();
        term?.focus();
      });
    }
  });
</script>

<div class="relative h-full w-full">
  <!-- FitAddon subtracts this padding, so the grid never overlaps it. -->
  <div
    class="h-full w-full bg-screen pt-1 pl-2"
    bind:this={host}
    onmousedowncapture={onMouseDown}
    oncontextmenucapture={onContextMenu}
  ></div>
  {#if tab.find.open}
    <TerminalFindBar
      find={tab.find}
      limit={FIND_LIMIT}
      bind:ref={findField}
      onstep={(forward) => step(forward)}
      onclose={closeFind}
    />
  {/if}
</div>
{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onclose={() => (menu = null)} />
{/if}
