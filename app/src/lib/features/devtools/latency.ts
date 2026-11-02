import { commands } from '../../bindings';
import type { OpenRequest } from '../../ipc';
import { workspace, type SessionTab } from '../../sessions.svelte';
import { saved } from '../tree/saved.svelte';
import type { FrameProbe, PaintTimes, PieceRect, TerminalProbe } from './latencyProbes';
import { inversions, LATENCY_OSC, summarise } from './latencyStats';

type Stages = Record<string, number>;

const PROBES = 200;
const ECHO_TIMEOUT_MS = 1000;
const SETTLE_MS = 3000;
const BEL = 7;
const BURST = 500;
const MANY_TABS = 24;
const HIDDEN_MS = 20_000;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const now = () => performance.now();

class FrameTrace implements FrameProbe {
  samples: Stages[] = [];
  timeouts = 0;
  #open: {
    input: number;
    sent: number;
    echoed: (rects: PieceRect[]) => boolean;
    stages?: Stages;
    finish: () => void;
  } | null = null;

  // A key goes out from its event; a move waits for the view's animation frame.
  one(input: () => void, sentAtOnce: boolean, echoed: (rects: PieceRect[]) => boolean) {
    return new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        this.timeouts++;
        this.#open = null;
        resolve();
      }, ECHO_TIMEOUT_MS);
      const at = now();
      this.#open = {
        input: at,
        sent: sentAtOnce ? at : 0,
        echoed,
        finish: () => {
          clearTimeout(timer);
          this.#open = null;
          resolve();
        },
      };
      input();
    });
  }

  sent() {
    if (this.#open && !this.#open.sent) this.#open.sent = now();
  }

  painted(times: PaintTimes, rects: PieceRect[]) {
    const open = this.#open;
    if (!open || !open.sent || open.stages || !open.echoed(rects)) return;
    open.stages = {
      wait_flush: open.sent - open.input,
      to_ui: times.received - open.sent,
      decode: times.painting - times.received,
      draw: times.drawn - times.painting,
      input_to_drawn: times.drawn - open.input,
    };
    // What's drawn between animation frames goes out with the next update.
    requestAnimationFrame(() => {
      open.stages!.input_to_shown = now() - open.input;
      this.samples.push(open.stages!);
      open.finish();
    });
  }
}

class TerminalTrace implements TerminalProbe {
  samples: Stages[] = [];
  timeouts = 0;
  lastReceived = 0;
  lastWritten = 0;
  echoes: number[] = [];
  #open: {
    input: number;
    marked: boolean;
    received?: number;
    parsed?: number;
    writes: number;
    finish: () => void;
  } | null = null;
  #writesAhead = 0;

  one(input: () => void, marked: boolean) {
    return new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        this.timeouts++;
        this.#open = null;
        resolve();
      }, ECHO_TIMEOUT_MS);
      this.#open = {
        input: now(),
        marked,
        writes: 0,
        finish: () => {
          clearTimeout(timer);
          this.#open = null;
          resolve();
        },
      };
      input();
    });
  }

  received(bytes: Uint8Array) {
    this.lastReceived = now();
    this.#writesAhead++;
    const open = this.#open;
    if (!open || open.received !== undefined) return;
    if (open.marked && !bytes.includes(BEL)) return;
    open.received = this.lastReceived;
    open.writes = this.#writesAhead;
  }

  written() {
    this.lastWritten = now();
    this.#writesAhead--;
    const open = this.#open;
    if (!open || open.marked || open.received === undefined) return;
    if (--open.writes === 0) open.parsed = this.lastWritten;
  }

  echoParsed(data: string) {
    this.echoes.push(Number(data));
    if (this.#open?.marked && this.#open.received !== undefined) this.#open.parsed = now();
  }

  rendered() {
    const open = this.#open;
    if (!open || open.parsed === undefined || open.received === undefined) return;
    const { input, received, parsed, finish } = open;
    const rendered = now();
    requestAnimationFrame(() => {
      this.samples.push({
        to_ui: received - input,
        parse: parsed - received,
        wait_render: rendered - parsed,
        input_to_rendered: rendered - input,
      });
      finish();
    });
    this.#open = { ...open, parsed: undefined, finish };
  }
}

async function connected(tab: SessionTab, fixture: boolean) {
  const answered = new Set<number>();
  for (let waited = 0; waited < 30_000; waited += 50) {
    const status = tab.status;
    if (status.state === 'connected' && tab.drive) return true;
    if (status.state === 'closed') return false;
    if (status.state === 'asking' && fixture && !answered.has(status.id)) {
      answered.add(status.id);
      if (status.prompt.kind === 'trust') tab.answer(status.id, { type: 'trust', trust: true });
      else if (status.prompt.kind === 'password') {
        tab.answer(status.id, { type: 'password', password: 'ela', remember: false });
      } else return false;
    }
    await sleep(50);
  }
  return false;
}

// What every input shares, measured beside the probes: the round trip of a command that does
// nothing, and how late the webview's main thread runs a timer.
function background(tab: SessionTab) {
  const samples: Stages[] = [];
  let running = true;
  void (async () => {
    while (running && tab.id) {
      const asked = now();
      await commands.sessionInput(tab.id, { type: 'release_all' });
      const answered = now();
      await sleep(50);
      samples.push({
        ipc_round_trip: answered - asked,
        main_thread_late: Math.max(0, now() - answered - 50),
      });
    }
  })();
  return () => {
    running = false;
    return samples;
  };
}

async function gap() {
  await sleep(30 + Math.random() * 50);
}

async function record(tab: SessionTab, name: string, stages: Stages) {
  await commands.recordBenchmark({
    run: 'input-latency',
    name,
    renderer: tab.renderer,
    bytes_per_sec: tab.meter.bytesPerSec,
    paints_per_sec: tab.meter.paintsPerSec,
    produced_per_sec: tab.statsRate?.produced ?? null,
    sent_per_sec: tab.statsRate?.sent ?? null,
    stages_ms: stages,
  });
}

// `?latency=key|flood` runs only the cases with one of those in their name.
function wanted(name: string) {
  const only = new URLSearchParams(location.search).get('latency');
  return !only || only === '1' || only.split('|').some((part) => name.includes(part));
}

async function withTab(
  name: string,
  request: OpenRequest,
  body: (tab: SessionTab) => Promise<{ samples: Stages[]; extra?: Stages }>,
  fixture = false,
) {
  if (!wanted(name)) return;
  const tab = workspace.open(request, { name: `Latency: ${name}`, env: 'lab' });
  try {
    if (!(await connected(tab, fixture))) {
      await commands.recordBenchmark({
        run: 'input-latency',
        name: `${name} (skipped: did not connect)`,
        renderer: tab.renderer,
        bytes_per_sec: 0,
        paints_per_sec: 0,
        produced_per_sec: null,
        sent_per_sec: null,
        stages_ms: {},
      });
      return;
    }
    await sleep(SETTLE_MS);
    const stopBackground = background(tab);
    const { samples, extra } = await body(tab);
    const shared = summarise(stopBackground());
    await record(tab, name, {
      ...summarise(samples),
      ...shared,
      ...tab.backendStages,
      samples: samples.length,
      ...extra,
    });
  } finally {
    tab.frameProbe = null;
    tab.terminalProbe = null;
    workspace.closeTab(tab.key);
    await sleep(500);
  }
}

function markChanged(mark: () => number) {
  let last = mark();
  return () => {
    const value = mark();
    if (value === last) return false;
    last = value;
    return true;
  };
}

async function pointerMarked(tab: SessionTab) {
  const trace = new FrameTrace();
  tab.frameProbe = trace;
  const { pointer, mark } = tab.drive!;
  const echoed = markChanged(mark!);
  for (let i = 0; i < PROBES; i++) {
    await gap();
    await trace.one(() => pointer!(i), false, echoed);
  }
  return { samples: trace.samples, extra: { timeouts: trace.timeouts } };
}

async function keyMarked(tab: SessionTab) {
  const trace = new FrameTrace();
  tab.frameProbe = trace;
  const { key, mark } = tab.drive!;
  const echoed = markChanged(mark!);
  for (let i = 0; i < PROBES; i++) {
    await gap();
    await trace.one(() => key!(true), true, echoed);
    key!(false);
  }
  return { samples: trace.samples, extra: { timeouts: trace.timeouts } };
}

// Without a mark to read, a move's echo would be a paint over where it went. The cursor is local
// since it stopped being painted, so a run of misses is the expected result.
async function pointerOnDesktop(tab: SessionTab) {
  const trace = new FrameTrace();
  tab.frameProbe = trace;
  const pointer = tab.drive!.pointer!;
  for (let i = 0; i < PROBES; i++) {
    if (trace.timeouts >= 5 && !trace.samples.length) break;
    await gap();
    let to = { x: 0, y: 0 };
    await trace.one(
      () => (to = pointer(i)),
      false,
      (rects) =>
        rects.some((r) => r.x <= to.x && to.x < r.x + r.w && r.y <= to.y && to.y < r.y + r.h),
    );
  }
  return { samples: trace.samples, extra: { timeouts: trace.timeouts } };
}

async function typedMarked(tab: SessionTab) {
  const trace = new TerminalTrace();
  tab.terminalProbe = trace;
  const type = tab.drive!.type!;
  for (let i = 0; i < PROBES; i++) {
    await gap();
    await trace.one(() => type(`x\x1b]${LATENCY_OSC};${i}\x07`), true);
  }
  return { samples: trace.samples, extra: { timeouts: trace.timeouts } };
}

async function typedInShell(tab: SessionTab) {
  const trace = new TerminalTrace();
  tab.terminalProbe = trace;
  const type = tab.drive!.type!;
  for (let i = 0; i < PROBES; i++) {
    await gap();
    await trace.one(() => type('x'), false);
    if (i % 60 === 59) {
      type('\x15');
      await sleep(200);
    }
  }
  type('\x15');
  await sleep(200);
  return { samples: trace.samples, extra: { timeouts: trace.timeouts } };
}

async function untilOutputStops(tab: SessionTab, trace: TerminalTrace) {
  const type = tab.drive!.type!;
  const flood_mb_per_s = tab.meter.bytesPerSec / 1_000_000;
  const pressed = now();
  type('\x03');
  while (now() - Math.max(trace.lastReceived, trace.lastWritten) < 500) await sleep(20);
  return {
    flood_mb_per_s,
    ctrl_c_to_last_received: trace.lastReceived - pressed,
    ctrl_c_to_last_parsed: trace.lastWritten - pressed,
  };
}

async function floodThenCtrlC(tab: SessionTab) {
  const typed = await typedMarked(tab);
  const stopped = await untilOutputStops(tab, tab.terminalProbe as TerminalTrace);
  return { samples: typed.samples, extra: { ...typed.extra, ...stopped } };
}

async function shellThenCtrlC(tab: SessionTab) {
  const typed = await typedInShell(tab);
  const trace = tab.terminalProbe as TerminalTrace;
  tab.drive!.type!('yes\n');
  await sleep(SETTLE_MS);
  const stopped = await untilOutputStops(tab, trace);
  return { samples: typed.samples, extra: { ...typed.extra, ...stopped } };
}

async function burstOrder(tab: SessionTab) {
  const trace = new TerminalTrace();
  tab.terminalProbe = trace;
  for (let i = 0; i < BURST; i++) {
    tab.input({ type: 'text', data: `\x1b]${LATENCY_OSC};${i}\x07` });
  }
  for (let waited = 0; waited < 5000 && trace.echoes.length < BURST; waited += 50) await sleep(50);
  return {
    samples: [],
    extra: {
      burst_sent: BURST,
      burst_echoed: trace.echoes.length,
      burst_out_of_order: inversions(trace.echoes),
    },
  };
}

async function hiddenTabCost() {
  const hidden = workspace.open(framebuffer('full'), { name: 'Latency: hidden', env: 'lab' });
  await sleep(SETTLE_MS);
  const shown = workspace.open(terminal('idle'), { name: 'Latency: shown', env: 'lab' });
  await sleep(HIDDEN_MS);
  await record(hidden, 'hidden Tab', {
    hidden_paints_per_s: hidden.meter.paintsPerSec,
    hidden_mb_per_s: hidden.meter.bytesPerSec / 1_000_000,
  });
  const before = hidden.meter.paintsPerSec;
  workspace.select(hidden.key);
  await sleep(1500);
  await record(hidden, 'hidden Tab, shown again', {
    paints_per_s_when_hidden: before,
    paints_per_s_when_shown: hidden.meter.paintsPerSec,
  });
  workspace.closeTab(shown.key);
  workspace.closeTab(hidden.key);
  await sleep(500);
}

async function manyTerminals() {
  const tabs = Array.from({ length: MANY_TABS }, (_, i) =>
    workspace.open(terminal('idle'), { name: `Latency: Tab ${i + 1}`, env: 'lab' }),
  );
  await sleep(SETTLE_MS);
  const lost = tabs.filter((tab) => tab.drawBackend !== 'webgl').length;
  let activeWithoutWebgl = 0;
  for (const tab of tabs) {
    workspace.select(tab.key);
    await sleep(200);
    if (tab.drawBackend !== 'webgl') activeWithoutWebgl++;
  }
  await record(tabs[0], 'terminal, many Tabs', {
    tabs: tabs.length,
    webgl_lost_behind: lost,
    active_without_webgl: activeWithoutWebgl,
  });
  for (const tab of tabs) workspace.closeTab(tab.key);
  await sleep(500);
}

function framebuffer(mode: string): OpenRequest {
  return {
    protocol: 'bench-fb',
    host: 'bench',
    options: { mode, echo: true, width: 1920, height: 1080, fps: 60 },
  };
}

function terminal(echo: 'idle' | 'flood'): OpenRequest {
  return { protocol: 'bench-term', host: 'bench', options: { echo } };
}

export async function measureInputLatency() {
  if (!workspace.protocols.length) await workspace.load();
  saved.showToast({ message: 'Measuring input latency. Hands off until it finishes.' });

  const loads: [string, string][] = [
    ['idle', 'idle'],
    ['moving box', 'partial'],
    ['every pixel', 'full'],
    ['lossless pass', 'bursts'],
  ];
  for (const [label, mode] of loads) {
    await withTab(`pointer, ${label}`, framebuffer(mode), pointerMarked);
    await withTab(`key, ${label}`, framebuffer(mode), keyMarked);
  }
  await withTab('terminal, idle', terminal('idle'), typedMarked);
  await withTab('terminal, flood', terminal('flood'), floodThenCtrlC);
  await withTab('input order', terminal('idle'), burstOrder);
  if (wanted('terminal, many Tabs')) await manyTerminals();

  if (wanted('busy Tab behind')) {
    const busy = workspace.open(framebuffer('full'), { name: 'Latency: busy Tab', env: 'lab' });
    await sleep(SETTLE_MS);
    await withTab('terminal, busy Tab behind', terminal('idle'), typedMarked);
    await withTab('pointer, busy Tab behind', framebuffer('idle'), pointerMarked);
    workspace.closeTab(busy.key);
  }

  await withTab(
    'SSH test server, shell',
    { protocol: 'ssh', host: 'localhost', port: 2222, username: 'ela' },
    shellThenCtrlC,
    true,
  );
  await withTab(
    'RDP test desktop, pointer',
    { protocol: 'rdp', host: 'localhost', port: 3389, username: 'ela' },
    pointerOnDesktop,
    true,
  );

  if (wanted('hidden Tab')) await hiddenTabCost();

  await commands.recordBenchmark({
    run: 'input-latency',
    name: 'finished',
    renderer: 'none',
    bytes_per_sec: 0,
    paints_per_sec: 0,
    produced_per_sec: null,
    sent_per_sec: null,
    stages_ms: {},
  });
  saved.showToast({
    message: 'Input latency measured',
    detail: 'Results in logs/benchmarks.jsonl',
  });
}
