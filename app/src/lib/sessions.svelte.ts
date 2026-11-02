import { Channel } from '@tauri-apps/api/core';
import { commands } from './bindings';
import { copyText, pasteText } from './clipboard';
import { TerminalFind } from './features/sessions/find.svelte';
import { summariseLinks } from './features/sessions/link';
import { advance, START, type Progress } from './features/sessions/progress';
import type { Drive, FrameProbe, TerminalProbe } from './features/devtools/latencyProbes';
import type { Env, LinkState } from './ui';
import type {
  FrontendEvent,
  OpenRequest,
  PromptReply,
  ProtocolInfo,
  RemoteCursor,
  RendererKind,
  SessionCommand,
  SessionStats,
  SessionStatus,
} from './ipc';

class Meter {
  bytesPerSec = $state(0);
  paintsPerSec = $state(0);
  #bytes = 0;
  #paints = 0;
  #stageTotals: Record<string, number> = {};
  #rafTotal = 0;
  #rafCount = 0;
  stages: Record<string, number> = {};
  onSample: (() => void) | null = null;
  #timer = setInterval(() => {
    this.stages = Object.fromEntries(
      Object.entries(this.#stageTotals).map(([stage, total]) => [
        stage,
        total / Math.max(this.#paints, 1),
      ]),
    );
    if (this.#rafCount) this.stages.raf_interval = this.#rafTotal / this.#rafCount;
    this.#stageTotals = {};
    this.#rafTotal = 0;
    this.#rafCount = 0;
    this.bytesPerSec = this.#bytes;
    this.paintsPerSec = this.#paints;
    this.#bytes = 0;
    this.#paints = 0;
    this.onSample?.();
  }, 1000);

  received(bytes: number) {
    this.#bytes += bytes;
  }
  painted() {
    this.#paints++;
  }
  stage(name: string, ms: number) {
    this.#stageTotals[name] = (this.#stageTotals[name] ?? 0) + ms;
  }
  animationFrame(ms: number) {
    this.#rafTotal += ms;
    this.#rafCount++;
  }
  dispose() {
    clearInterval(this.#timer);
  }
}

export interface TabMeta {
  name?: string;
  env?: Env;
  connection?: string;
  record?: boolean;
}

export class SessionTab {
  readonly key = crypto.randomUUID();
  readonly renderer: RendererKind;
  readonly protocol: string;
  readonly host: string;
  readonly port: number | null;
  readonly name: string;
  readonly user: string | null;
  readonly env: Env;
  readonly connection: string | null;
  readonly address: string;
  readonly meta: TabMeta;
  readonly reopenRequest: OpenRequest;
  activity = $state(0);

  id = $state<string | null>(null);
  status = $state<SessionStatus>({ state: 'idle' });
  stats = $state<SessionStats | null>(null);
  fingerprint = $state<string | null>(null);
  progress = $state<Progress>(START);
  // Every reconnect opens a new Tab, so a Tab that never connected has no screen to keep.
  everConnected = $state(false);
  readonly startedAt = Date.now();
  drawBackend = $state('');
  keyboardReleased = $state(false);
  statsOpen = $state(false);
  find = new TerminalFind();
  displayMode = $state<'dynamic' | 'fit' | 'original'>('dynamic');
  clipboardSync = $state(true);
  readonly meter = new Meter();

  #request: OpenRequest | null;
  #pendingAcks = 0;
  #keepingPassword = false;
  /** What the clipboard held last time, so regaining focus doesn't replace the remote one. */
  #clipboardSeen: string | null = null;
  #prevStats: SessionStats | null = null;
  statsRate = $state<{ produced: number; sent: number } | null>(null);
  #backendStages: Record<string, number> = {};
  remoteCursor = $state<RemoteCursor>({ shape: 'default' });
  drive: Drive | null = null;
  frameProbe: FrameProbe | null = null;
  terminalProbe: TerminalProbe | null = null;

  constructor(request: OpenRequest, renderer: RendererKind, meta: TabMeta = {}) {
    this.#request = request;
    this.reopenRequest = request;
    this.meta = meta;
    this.renderer = renderer;
    this.protocol = request.protocol;
    // Benchmarks exist to be measured, so their readout starts open.
    this.statsOpen = request.protocol.startsWith('bench');
    this.host = request.host;
    this.port = request.port ?? null;
    this.user = request.username ?? null;
    this.name = meta.name ?? request.host;
    this.env = meta.env ?? 'none';
    this.connection = meta.connection ?? null;
    const port = request.port ? `:${request.port}` : '';
    this.address = `${this.user ? `${this.user}@` : ''}${request.host}${port}`;
    if (meta.record) this.meter.onSample = () => this.#recordSample();
  }

  get linkState(): LinkState {
    switch (this.status.state) {
      case 'connected':
        return 'connected';
      case 'reconnecting':
        return 'reconnecting';
      case 'asking':
        return 'waiting';
      case 'closed':
        return this.status.error ? 'failed' : 'closed';
      default:
        return 'connecting';
    }
  }

  async start(size: { width: number; height: number }, onData: (msg: ArrayBuffer) => void) {
    const request = this.#request;
    if (!request) return;
    this.#request = null;

    // The generator types channels by the direction of arguments, but these two carry
    // messages from Rust: serialized events, and raw bytes as an ArrayBuffer.
    const events = new Channel<FrontendEvent>((ev) => this.#onEvent(ev));
    const data = new Channel<number[]>((raw) => {
      const msg = raw as unknown as ArrayBuffer;
      this.meter.received(msg.byteLength);
      const now = performance.now();
      if (now - this.activity > 150) this.activity = now;
      onData(msg);
    });
    this.status = { state: 'connecting', jump_host: null };
    try {
      this.id = this.connection
        ? await commands.openSavedSession({ id: this.connection, size }, events as never, data)
        : await commands.openSession({ ...request, size }, events as never, data);
      for (; this.#pendingAcks > 0; this.#pendingAcks--) this.ack();
    } catch (e) {
      this.status = { state: 'closed', error: { kind: 'protocol', detail: String(e) } };
    }
  }

  get backendStages(): Readonly<Record<string, number>> {
    return this.#backendStages;
  }

  #recordSample() {
    if (this.status.state !== 'connected') return;
    const sample = {
      run: this.key,
      name: this.name,
      renderer: this.renderer,
      bytes_per_sec: this.meter.bytesPerSec,
      paints_per_sec: this.meter.paintsPerSec,
      produced_per_sec: this.statsRate?.produced ?? null,
      sent_per_sec: this.statsRate?.sent ?? null,
      stages_ms: { ...this.meter.stages, ...this.#backendStages },
    };
    commands.recordBenchmark(sample).catch((e) => console.warn('benchmark log failed', e));
  }

  /** Tells the remote side about new local text, without sending the text itself. */
  async offerClipboard() {
    if (!this.clipboardSync) return;
    const text = await pasteText();
    if (text === null || text === this.#clipboardSeen) return;
    this.#clipboardSeen = text;
    this.input({ type: 'clipboard_offer' });
  }

  #onEvent(ev: FrontendEvent) {
    if (ev.type === 'clipboard_write') {
      if (!this.clipboardSync) return;
      this.#clipboardSeen = ev.text;
      void copyText(ev.text);
    } else if (ev.type === 'clipboard_read') {
      if (!this.clipboardSync) return this.input({ type: 'clipboard_text', text: null });
      void pasteText().then((text) => this.input({ type: 'clipboard_text', text }));
    } else if (ev.type === 'cursor') {
      this.remoteCursor = ev.cursor;
    } else if (ev.type === 'status') {
      this.status = ev.status;
      this.progress = advance(this.progress, ev.status);
      // Rust keeps a remembered password before it moves on from that Jump Host.
      const signingIn =
        ev.status.state === 'authenticating' ||
        (ev.status.state === 'asking' && ev.status.prompt.kind === 'password');
      if (this.#keepingPassword && !signingIn) {
        this.#keepingPassword = false;
        workspace.onSavedChanged();
      }
      if (ev.status.state === 'connected') this.everConnected = true;
      if (ev.status.state === 'verifying' && !ev.status.jump_host) {
        this.fingerprint = ev.status.fingerprint;
      }
    } else {
      const prev = this.#prevStats;
      if (prev) {
        const sent = ev.stats.sent - prev.sent;
        this.statsRate = { produced: ev.stats.produced - prev.produced, sent };
        if (sent > 0 && this.renderer === 'framebuffer') {
          const perFrame = (now: number, before: number) => (now - before) / sent / 1_000_000;
          this.#backendStages = {
            take: perFrame(ev.stats.take_ns, prev.take_ns),
            encode: perFrame(ev.stats.encode_ns, prev.encode_ns),
            send: perFrame(ev.stats.send_ns, prev.send_ns),
            ack_wait: perFrame(ev.stats.ack_wait_ns, prev.ack_wait_ns),
            credit_wait: perFrame(ev.stats.credit_wait_ns, prev.credit_wait_ns),
          };
        }
      }
      this.#prevStats = ev.stats;
      this.stats = ev.stats;
    }
  }

  ack() {
    if (!this.id) {
      this.#pendingAcks++;
      return;
    }
    void commands.sessionAck(this.id);
  }

  saveScreenshot() {
    return commands.saveScreenshot(this.id!);
  }

  sendCtrlAltDel() {
    for (const code of ['ControlLeft', 'AltLeft', 'Delete'])
      this.input({ type: 'key', code, down: true });
    for (const code of ['Delete', 'AltLeft', 'ControlLeft'])
      this.input({ type: 'key', code, down: false });
  }

  input(command: SessionCommand) {
    if (!this.id || this.status.state === 'closed') return;
    void commands.sessionInput(this.id, command);
  }

  /** Names the prompt it answers, so a late or doubled answer can't reach the next one. */
  answer(prompt: number, reply: PromptReply) {
    if (!this.id) return;
    if (reply.type === 'password') this.#keepingPassword ||= reply.remember;
    void commands.sessionAnswer(this.id, prompt, reply);
  }

  cancel() {
    if (this.id) void commands.closeSession(this.id);
  }

  close() {
    if (this.id) void commands.closeSession(this.id);
    this.meter.dispose();
  }
}

class Workspace {
  tabs = $state<SessionTab[]>([]);
  activeKey = $state<string | null>(null);
  protocols = $state<ProtocolInfo[]>([]);
  onSavedChanged: () => void = () => {};
  onSelected: () => void = () => {};

  get active(): SessionTab | undefined {
    return this.tabs.find((t) => t.key === this.activeKey);
  }

  async load() {
    this.protocols = await commands.listProtocols();
  }

  select(key: string) {
    this.activeKey = key;
    this.onSelected();
  }

  protocol(id: string) {
    return this.protocols.find((p) => p.id === id);
  }

  linkOf(connection: string) {
    return summariseLinks(
      this.tabs.filter((tab) => tab.connection === connection).map((tab) => tab.linkState),
    );
  }

  open(request: OpenRequest, meta: TabMeta = {}, replace?: string) {
    const info = this.protocol(request.protocol);
    if (!info) throw new Error(`Ela doesn't support the “${request.protocol}” protocol yet.`);
    const tab = new SessionTab(request, info.renderer, meta);
    const index = this.tabs.findIndex((t) => t.key === replace);
    if (index >= 0) {
      this.tabs[index].close();
      this.tabs[index] = tab;
    } else {
      this.tabs.push(tab);
    }
    this.select(tab.key);
    return tab;
  }

  reopen(tab: SessionTab) {
    return this.open(tab.reopenRequest, tab.meta, tab.key);
  }

  cycle(step: 1 | -1) {
    if (!this.tabs.length) return;
    const i = this.tabs.findIndex((t) => t.key === this.activeKey);
    this.select(this.tabs[(i + step + this.tabs.length) % this.tabs.length].key);
  }

  closeTab(key: string) {
    const i = this.tabs.findIndex((t) => t.key === key);
    if (i < 0) return;
    this.tabs[i].close();
    this.tabs.splice(i, 1);
    if (this.activeKey === key)
      this.activeKey = this.tabs[Math.min(i, this.tabs.length - 1)]?.key ?? null;
  }
}

export const workspace = new Workspace();
