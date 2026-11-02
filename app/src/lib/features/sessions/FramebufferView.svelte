<script lang="ts">
  import { onMount } from 'svelte';
  import { ENCODING_JPEG, FRAME_HEADER_LEN, FRAME_RECT_LEN, MSG_FRAME } from '../../ipc';
  import type { SessionTab } from '../../sessions.svelte';
  import { runCommand, shell } from '../../shell/shell.svelte';
  import { hostKey, type KeyDecision } from '../../shell/shortcuts';

  const RESIZE_DEBOUNCE_MS = 300;

  type Piece = { x: number; y: number; image: ImageData | ImageBitmap };
  type DecodedFrame = { width: number; height: number; pieces: Piece[] };

  let { tab, active }: { tab: SessionTab; active: boolean } = $props();

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let displayW = $state(0);
  let displayH = $state(0);

  // With one credit in flight at most one frame waits here; the backend merges everything else
  // (ADR-0007).
  let pending: DecodedFrame | null = null;
  let pendingSince = 0;
  let pendingMove: { x: number; y: number } | null = null;

  function layout() {
    if (!canvas.width || !container.clientWidth) return;
    const scale =
      tab.displayMode === 'original'
        ? 1 / (window.devicePixelRatio || 1)
        : Math.min(container.clientWidth / canvas.width, container.clientHeight / canvas.height);
    displayW = Math.floor(canvas.width * scale);
    displayH = Math.floor(canvas.height * scale);
  }

  async function decode(msg: ArrayBuffer): Promise<DecodedFrame | null> {
    const v = new DataView(msg);
    if (v.getUint8(0) !== MSG_FRAME) return null;
    const [width, height, count] = [1, 3, 5].map((o) => v.getUint16(o, true));
    const pieces: (Piece | Promise<Piece>)[] = [];
    let offset = FRAME_HEADER_LEN + count * FRAME_RECT_LEN;
    for (let i = 0; i < count; i++) {
      const at = FRAME_HEADER_LEN + i * FRAME_RECT_LEN;
      const [x, y, w, h] = [0, 2, 4, 6].map((o) => v.getUint16(at + o, true));
      const len = v.getUint32(at + 9, true);
      if (v.getUint8(at + 8) === ENCODING_JPEG) {
        const blob = new Blob([new Uint8Array(msg, offset, len)], { type: 'image/jpeg' });
        pieces.push(
          createImageBitmap(blob, { colorSpaceConversion: 'none' }).then((image) => ({
            x,
            y,
            image,
          })),
        );
      } else {
        pieces.push({ x, y, image: new ImageData(new Uint8ClampedArray(msg, offset, len), w, h) });
      }
      offset += len;
    }
    return { width, height, pieces: await Promise.all(pieces) };
  }

  function paint(ctx: CanvasRenderingContext2D, frame: DecodedFrame) {
    if (canvas.width !== frame.width || canvas.height !== frame.height) {
      canvas.width = frame.width;
      canvas.height = frame.height;
      layout();
    }
    for (const { x, y, image } of frame.pieces) {
      if (image instanceof ImageData) {
        ctx.putImageData(image, x, y);
      } else {
        ctx.drawImage(image, x, y);
        image.close();
      }
    }
  }

  function toRemote(e: MouseEvent) {
    const r = canvas.getBoundingClientRect();
    return {
      x: Math.max(0, Math.round(((e.clientX - r.left) / r.width) * canvas.width)),
      y: Math.max(0, Math.round(((e.clientY - r.top) / r.height) * canvas.height)),
    };
  }

  onMount(() => {
    const ctx = canvas.getContext('2d', { alpha: false, desynchronized: true })!;
    tab.drawBackend = 'canvas2d';
    let raf = 0;
    let lastFrame = 0;
    const loop = (frameAt: number) => {
      if (lastFrame) tab.meter.animationFrame(frameAt - lastFrame);
      lastFrame = frameAt;
      if (pending) {
        const frame = pending;
        pending = null;
        const painting = performance.now();
        tab.meter.stage('wait_for_frame', painting - pendingSince);
        paint(ctx, frame);
        tab.meter.stage('draw', performance.now() - painting);
        tab.meter.painted();
        const acking = performance.now();
        tab.ack();
        tab.meter.stage('ack_call', performance.now() - acking);
      }
      if (pendingMove) {
        tab.input({ type: 'mouse_move', ...pendingMove });
        pendingMove = null;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    const dpr = window.devicePixelRatio || 1;
    void tab.start(
      {
        width: Math.round(container.clientWidth * dpr),
        height: Math.round(container.clientHeight * dpr),
      },
      (msg) => {
        const received = performance.now();
        decode(msg)
          .then((frame) => {
            if (!frame) return tab.ack();
            const decoded = performance.now();
            tab.meter.stage('decode', decoded - received);
            if (pending) tab.ack(); // never expected with one credit; keep the pipeline moving anyway
            pending = frame;
            pendingSince = decoded;
          })
          .catch((e) => {
            // Skip this update and keep the Session going; the next one may be fine.
            console.warn('could not decode a frame', e);
            tab.ack();
          });
      },
    );

    let resizeTimer: ReturnType<typeof setTimeout>;
    const ro = new ResizeObserver(() => {
      layout();
      // Dragging a window edge fires constantly, and each new size makes the server repaint.
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(askForViewSize, RESIZE_DEBOUNCE_MS);
    });
    ro.observe(container);
    // Svelte registers wheel listeners as passive; preventDefault needs an active one.
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      clearTimeout(resizeTimer);
      canvas.removeEventListener('wheel', onWheel);
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  });

  // A derived flag, so other status changes don't pull focus away from where the user is.
  const awaitingUser = $derived(tab.linkState === 'waiting');

  // Focus comes back here whenever this Tab shows, and when a panel like the first-connection
  // one, which holds focus meanwhile, gives way to the connected Session.
  $effect(() => {
    if (active && !awaitingUser && !tab.keyboardReleased && !shell.sheetOpen && !shell.palette) {
      requestAnimationFrame(() => canvas.focus());
    }
  });

  function askForViewSize() {
    if (tab.displayMode !== 'dynamic') return;
    const scale = window.devicePixelRatio || 1;
    const [width, height] = [container.clientWidth, container.clientHeight].map((n) =>
      Math.round(n * scale),
    );
    if (width && height) tab.input({ type: 'resize', width, height });
  }

  $effect(() => {
    void tab.displayMode;
    layout();
    askForViewSize();
  });

  function act(e: KeyboardEvent, down: boolean, decision: KeyDecision) {
    if (decision.kind === 'forward') tab.input({ type: 'key', code: e.code, down });
    else if (decision.kind === 'command') runCommand(decision.command);
    else if (decision.kind === 'tap') tab.keyboardReleased = !tab.keyboardReleased;
  }

  // Keys here never reach the app's own shortcuts: only the host key takes them back.
  function onKeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    act(e, true, hostKey.keydown(e.code, e.repeat));
  }

  function onKeyup(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    act(e, false, hostKey.keyup(e.code));
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    tab.input({ type: 'wheel', vertical: -Math.sign(e.deltaY), horizontal: Math.sign(e.deltaX) });
  }
</script>

<div class="relative h-full w-full bg-screen">
  <!-- 1:1 can be larger than the view, and then scrolls. `m-auto` centres without clipping. -->
  <div
    class="flex h-full w-full {tab.displayMode === 'original'
      ? 'overflow-auto'
      : 'overflow-hidden'}"
    bind:this={container}
  >
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <canvas
      bind:this={canvas}
      tabindex="0"
      class="m-auto shrink-0 outline-none"
      style:width="{displayW}px"
      style:height="{displayH}px"
      onkeydown={onKeydown}
      onkeyup={onKeyup}
      onfocus={() => {
        tab.keyboardReleased = false;
        void tab.offerClipboard();
      }}
      onblur={() => {
        hostKey.forgetForwarded();
        tab.input({ type: 'release_all' });
      }}
      onmousemove={(e) => (pendingMove = toRemote(e))}
      onmousedown={(e) => {
        canvas.focus();
        tab.input({ type: 'mouse_button', button: e.button, down: true, ...toRemote(e) });
      }}
      onmouseup={(e) =>
        tab.input({ type: 'mouse_button', button: e.button, down: false, ...toRemote(e) })}
      oncontextmenu={(e) => e.preventDefault()}
    ></canvas>
  </div>
</div>
