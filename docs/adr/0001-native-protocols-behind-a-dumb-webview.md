---
status: accepted; the spike's conditions are met (see Spike results)
---

# Native protocols behind a dumb webview

The UI is a Tauri webview, but every Protocol (SSH, RDP, VNC…) runs natively in Rust. The webview only paints terminal bytes and framebuffer rectangles and sends input back. We chose a webview over GTK4 for cross-platform reach and UI freedom. We rejected Remota's approach of running RDP/VNC inside the browser (ironrdp-web WASM, noVNC behind a local WebSocket gateway) because native clients make clipboard, drive redirection, audio and reconnect easier, give all Protocols one Session model, and drop the localhost gateway as an attack surface.

## Consequences

- The Linux webview is WebKitGTK, which is slower than Chromium at canvas and WebGL. A 1080p RDP + xterm.js spike must pass before the MVP. If it fails, `app/` is rewritten in GTK4 (VTE + `gtk::Picture`) and `crates/` are kept, which is why `app/src-tauri` must hold no protocol logic.
- Framebuffer pixels cross the IPC boundary, so dirty-rect updates and dropping stale frames are required (see ADR-0007).

## Spike results

Measured 2026-10-31 with the `ela-bench` Protocols. Setup: debug build, WebKitGTK, user's Linux desktop, raw RGBA over Tauri Channels, one frame in flight.

| Benchmark | Result |
|---|---|
| Terminal flood, 50 MB | 1.09 s = 45.9 MB/s |
| Terminal flood, 200 MB | 3.39 s = 59.0 MB/s |
| 1080p, every pixel changes | ~150 MB/s received, 21 fps painted, source 39 updates/s, ~50% merged away |
| 1080p, moving box | 80–200 MB/s received (inflated by bounding-box merging, fixed below), 40–60 fps painted, 0–2% merged |
| 4K, every pixel changes | ~180 MB/s received, 6–10 fps painted |

With several dirty rectangles (same setup): 1080p moving box at about 17 MB/s received, steady 60 fps painted.

Release build with several dirty rectangles, measured 2026-10-31 with `bun tauri dev --release`, same machine and transport:

| Benchmark | Result |
|---|---|
| Terminal flood, 50 MB | 1.27 s = 39.3 MB/s |
| Terminal flood, 200 MB | 4.17 s = 47.9 MB/s |
| 1080p, every pixel changes | 276 MB/s received, 36 fps painted, source 60 updates/s, 42–45% merged |
| 1080p, moving box | 17 MB/s received, 60 fps painted, source 60 updates/s, 0–2% merged |
| 4K, every pixel changes | 280–300 MB/s received, 9–10 fps painted, source 30 updates/s, 70% merged |

Release lifts the framebuffer ceiling from about 180 MB/s to about 280–300 MB/s. 1080p full-screen change already passes the 30 fps bar below with raw pixels; 4K does not reach usable motion. Terminal floods came out slower than in the debug runs. These are single runs and the cause was not investigated; both are still far above what an SSH link needs.

Transport A/B, measured 2026-10-31 in one release session. Medians per run, leaving out each run's first 3 seconds:

| Benchmark | Tauri Channel | Local WebSocket |
|---|---|---|
| 1080p, every pixel changes | 257 MB/s, 32.5 fps | 237 MB/s, 30 fps |
| 1080p, moving box | 17 MB/s, 60 fps | 17 MB/s, 60 fps |
| 4K, every pixel changes | 316 MB/s, 10 fps | 316 MB/s, 10 fps |

The WebSocket does not win, so framebuffer data stays on Channels and the experiment was removed. That also keeps out the localhost listener this ADR wanted to avoid. The next measurement shows where the time goes.

Time per frame, measured 2026-10-31 in one release session with a Tauri Channel. Medians per run in milliseconds, leaving out each run's first 3 seconds:

| Stage | 1080p moving box | 1080p every pixel | 4K every pixel |
|---|---|---|---|
| Rust: lock the surface, copy pixels (`take`) | 0.05 | 1.6 | 7.5 |
| Rust: hand to the Channel (`send`) | 0.02 | 0.02 | 0.02 |
| UI: wait for the animation frame | 13.7 | 2.5 | 2.5 |
| UI: build `ImageData` | 0.05 | 0.5 | 0.4 |
| UI: `putImageData` returns | 0.3 | 0.9 | 3.0 |
| UI: send the ack | 0.3 | 0.3 | 0.9 |
| **Not in any stage above: data on its way to the UI, and the ack back** | **2.4** | **23.6** | **90.2** |
| Whole cycle | 16.7 | 29.5 | 104.4 |

The missing time is worked out by subtraction: Rust's wait for the ack minus the UI stages. It is 80% of a 1080p frame and 86% of a 4K frame, and it grows with size at about 350–370 MB/s. Animation frames keep arriving every 17–22 ms meanwhile, so the webview's main thread is not what is blocked. Painting is not the limit, as far as these calls show: `putImageData` may defer some work until later, but that work doesn't hold back the next frame. The limit is moving bytes into the webview, and it costs about the same over a WebSocket. So the way to more fps is fewer bytes per frame, which is what JPEG for large updates does.

JPEG for large updates, measured 2026-10-31 in release builds. Rects covering at least a quarter of the surface go as JPEG quality 75, split into 4 bands encoded in parallel, and are sent again losslessly once they settle. Medians per run, leaving out each run's first 3 seconds:

| Benchmark | Lossless | JPEG |
|---|---|---|
| 1080p, every pixel changes | 261 MB/s, 33 fps | 8.9 MB/s, 60 fps |
| 1080p, moving box | not applicable, stays lossless | 17 MB/s, 60 fps |
| 4K, every pixel changes | 316 MB/s, 10 fps | 12.2 MB/s, 21 fps |

Time per frame with JPEG, in milliseconds:

| Stage | 1080p every pixel | 4K every pixel |
|---|---|---|
| Rust: lock the surface, copy pixels | 1.8 | 9.2 |
| Rust: encode | 4.6 | 13.6 |
| UI: decode | 5.5 | 17.2 |
| UI: draw | 1.3 | 3.7 |

The benchmark image is a smooth gradient and compresses well, about 155 KB per 1080p frame. Real video produces larger messages, so expect lower numbers there. On one thread, encoding took 12.7 ms at 1080p and 47 ms at 4K, which held 4K at 13 fps.

**Verdict: Tauri stays.** Terminals have far more headroom than any SSH link needs. Framebuffers hit a transport ceiling of about 180 MB/s for raw pixels, which is enough for normal desktop work but not for full-screen motion. The webview remains the UI on these conditions:

1. Dirty regions travel as several rectangles, not one bounding box.
2. The transport is A/B tested against a local WebSocket. Done: no gain, Channels stay.
3. Large changing areas are sent lossy and decoded in the webview. Done: JPEG, 60 fps at 1080p and 21 fps at 4K in the benchmark.

If the conditions fail to bring 1080p full-screen change to at least 30 fps in a release build, revisit the GTK4 alternative.

## Running the benchmarks again

Open the benchmarks from the sidebar in `bun tauri dev` (ideally `--release`) and read the throughput overlay:

| Benchmark | Watch | Good sign |
|---|---|---|
| Terminal flood | Final MB/s line, UI stays responsive, Ctrl+C works mid-flood | Tens of MB/s, no freeze |
| Full HD, every pixel | Painted fps | Close to 60 on a modern machine |
| Full HD, moving box | Painted fps, Merged away | 60 fps, low merge rate |
| 4K, every pixel | Painted fps | Smooth enough to be usable |

While a benchmark Tab is connected, the app also appends its throughput once per second to `~/.local/share/dev.ela.app/logs/benchmarks.jsonl`, so results don't have to be copied from the overlay.
