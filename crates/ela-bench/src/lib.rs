use std::time::{Duration, Instant};

use async_trait::async_trait;
use ela_session::{
    Protocol, Rect, RendererKind, ResolvedConfig, Session, SessionCommand, SessionError, SessionIo,
    SessionSink,
};
use serde_json::json;
use tokio::sync::mpsc;

const CTRL_C: u8 = 3;
const FLOOD_LINE: &str = "\x1b[32m  12345\x1b[0m the quick brown fox jumps over the lazy dog\r\n";
// The largest packet a russh channel delivers by default.
const FLOOD_CHUNK: usize = 32 * 1024;

// The latency run reads these cells from the canvas, so an input's echo can be told from load even
// inside a JPEG. Keep in step with the run's `markValue`.
const MARK_BITS: u16 = 8;
const MARK_CELL: u16 = 32;

pub struct BenchTerminal;

#[async_trait]
impl Protocol for BenchTerminal {
    fn id(&self) -> &'static str {
        "bench-term"
    }
    fn display_name(&self) -> &'static str {
        "Benchmark: terminal flood"
    }
    fn default_port(&self) -> Option<u16> {
        None
    }
    fn renderer(&self) -> RendererKind {
        RendererKind::Terminal
    }
    fn listed(&self) -> bool {
        false
    }
    fn settings_schema(&self) -> serde_json::Value {
        json!({
            "mb": { "type": "integer", "default": 50, "label": "Megabytes to print" },
            "echo": { "type": "enum", "values": ["idle", "flood"] }
        })
    }
    async fn connect(
        &self,
        config: ResolvedConfig,
        _: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError> {
        if let Some(load) = config.option_str("echo") {
            return Ok(Box::new(EchoTerminal {
                flooding: load == "flood",
            }));
        }
        Ok(Box::new(TermSession {
            megabytes: config.option_u64("mb").unwrap_or(50),
        }))
    }
}

struct TermSession {
    megabytes: u64,
}

#[async_trait]
impl Session for TermSession {
    async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
        let target = self.megabytes * 1024 * 1024;
        let start = Instant::now();
        let mut written = 0u64;
        let mut line = 0u64;
        while written < target {
            if io.cancel.is_cancelled() {
                return Err(SessionError::Cancelled);
            }
            let mut chunk = String::with_capacity(16 * 1024);
            while chunk.len() < 16 * 1024 {
                let color = 31 + (line % 7);
                chunk.push_str(&format!(
                    "\x1b[{color}m{line:>9}\x1b[0m the quick brown fox jumps over the lazy dog \x1b[1m0123456789\x1b[0m ~$ ls -la /var/log\r\n"
                ));
                line += 1;
            }
            written += chunk.len() as u64;
            io.sink.terminal(chunk.into_bytes()).await?;
        }
        let secs = start.elapsed().as_secs_f64();
        let summary = format!(
            "\r\n\x1b[1;32m✔ {} MB in {secs:.2}s = {:.1} MB/s ({line} lines)\x1b[0m\r\nType to test echo latency. Close the tab to end.\r\n",
            self.megabytes,
            self.megabytes as f64 / secs
        );
        io.sink.terminal(summary.into_bytes()).await?;

        loop {
            tokio::select! {
                cmd = io.commands.recv() => match cmd {
                    Some(SessionCommand::Text { data }) => {
                        let echoed = data.replace('\r', "\r\n");
                        io.sink.terminal(echoed.into_bytes()).await?;
                    }
                    Some(_) => {}
                    None => return Ok(()),
                },
                _ = io.cancel.cancelled() => return Err(SessionError::Cancelled),
            }
        }
    }
}

// Holds the shape of an SSH Session, with output in its own task, so an echo queues behind a flood
// as it does there.
struct EchoTerminal {
    flooding: bool,
}

#[async_trait]
impl Session for EchoTerminal {
    async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
        let (echo, mut echoes) = mpsc::unbounded_channel::<Vec<u8>>();
        let sink = io.sink.clone();
        let mut flooding = self.flooding;
        let writer = tokio::spawn(async move {
            let chunk = FLOOD_LINE
                .repeat(FLOOD_CHUNK / FLOOD_LINE.len())
                .into_bytes();
            loop {
                let echoed = if flooding {
                    echoes.try_recv().ok()
                } else {
                    echoes.recv().await
                };
                let written = match echoed {
                    Some(data) => {
                        flooding &= !data.contains(&CTRL_C);
                        sink.terminal(data).await
                    }
                    None if flooding => sink.terminal(chunk.clone()).await,
                    None => return,
                };
                if written.is_err() {
                    return;
                }
            }
        });
        let ended = loop {
            tokio::select! {
                cmd = io.commands.recv() => match cmd {
                    Some(SessionCommand::Text { data }) => {
                        let _ = echo.send(data.into_bytes());
                    }
                    Some(_) => {}
                    None => break Ok(()),
                },
                _ = io.cancel.cancelled() => break Err(SessionError::Cancelled),
            }
        };
        writer.abort();
        ended
    }
}

pub struct BenchFramebuffer;

#[async_trait]
impl Protocol for BenchFramebuffer {
    fn id(&self) -> &'static str {
        "bench-fb"
    }
    fn display_name(&self) -> &'static str {
        "Benchmark: framebuffer"
    }
    fn default_port(&self) -> Option<u16> {
        None
    }
    fn renderer(&self) -> RendererKind {
        RendererKind::Framebuffer
    }
    fn listed(&self) -> bool {
        false
    }
    fn settings_schema(&self) -> serde_json::Value {
        json!({
            "mode": { "type": "enum", "values": ["full", "partial", "idle", "bursts"], "default": "full" },
            "echo": { "type": "boolean", "default": false },
            "image_quality": { "type": "enum", "values": ["auto", "lossless"], "default": "auto" },
            "fps": { "type": "integer", "default": 60 },
            "width": { "type": "integer", "default": 1920 },
            "height": { "type": "integer", "default": 1080 }
        })
    }
    async fn connect(
        &self,
        config: ResolvedConfig,
        _: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError> {
        let dim =
            |key, default: u64| config.option_u64(key).unwrap_or(default).clamp(16, 4096) as u16;
        Ok(Box::new(FbSession {
            load: match config.option_str("mode") {
                Some("partial") => Load::Box,
                Some("idle") => Load::Nothing,
                Some("bursts") => Load::Bursts,
                _ => Load::EveryPixel,
            },
            echo: config.options.get("echo") == Some(&json!(true)),
            fps: config.option_u64("fps").unwrap_or(60).clamp(1, 240),
            width: dim("width", 1920),
            height: dim("height", 1080),
        }))
    }
}

#[derive(Clone, Copy)]
enum Load {
    EveryPixel,
    Box,
    Nothing,
    // Long enough apart that the lossless pass runs between them.
    Bursts,
}

struct FbSession {
    load: Load,
    echo: bool,
    fps: u64,
    width: u16,
    height: u16,
}

#[async_trait]
impl Session for FbSession {
    async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
        let frame = io.sink.frame();
        frame.resize(self.width, self.height);
        let mut tick = tokio::time::interval(Duration::from_micros(1_000_000 / self.fps));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut n: u32 = 0;
        let mut cursor = (0u16, 0u16);
        let mut mark = 0u8;
        let draw_mark_now = |mark| {
            if self.echo {
                frame.draw(|px, w, h| [draw_mark(px, w, h, mark)]);
            }
        };
        draw_mark_now(mark);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                cmd = io.commands.recv() => {
                    match cmd {
                        Some(SessionCommand::MouseMove { x, y }) => {
                            cursor = (x, y);
                            mark = mark.wrapping_add(1);
                            draw_mark_now(mark);
                        }
                        Some(SessionCommand::Key { down: true, .. }) => {
                            mark = mark.wrapping_add(1);
                            draw_mark_now(mark);
                        }
                        None => return Ok(()),
                        _ => {}
                    }
                    continue;
                }
                _ = io.cancel.cancelled() => return Err(SessionError::Cancelled),
            }
            n = n.wrapping_add(1);
            match self.load {
                Load::EveryPixel => frame.draw(|px, w, h| draw_full(px, w, h, n)),
                Load::Box => frame.draw(|px, w, h| draw_partial(px, w, h, n, cursor)),
                Load::Nothing => continue,
                Load::Bursts if n % 33 >= 9 => continue,
                Load::Bursts => frame.draw(|px, w, h| draw_full(px, w, h, n)),
            }
            draw_mark_now(mark);
        }
    }
}

fn draw_mark(px: &mut [u8], w: u16, h: u16, value: u8) -> Rect {
    let (width, height) = ((MARK_BITS * MARK_CELL).min(w), MARK_CELL.min(h));
    for y in 0..usize::from(height) {
        for x in 0..usize::from(width) {
            let bit = x / usize::from(MARK_CELL);
            let c = if value >> bit & 1 == 1 { 255 } else { 0 };
            let at = (y * usize::from(w) + x) * 4;
            px[at..at + 4].copy_from_slice(&[c, c, c, 255]);
        }
    }
    Rect::new(0, 0, width, height)
}

fn draw_full(px: &mut [u8], w: u16, h: u16, n: u32) -> [Rect; 1] {
    let shift = (n * 4) as usize;
    for (y, row) in px.chunks_exact_mut(usize::from(w) * 4).enumerate() {
        for (x, p) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let v = x + y + shift;
            p.copy_from_slice(&[
                (v & 0xff) as u8,
                ((v >> 1) & 0xff) as u8,
                ((x ^ y) & 0xff) as u8,
                0xff,
            ]);
        }
    }
    [Rect::new(0, 0, w, h)]
}

fn draw_partial(px: &mut [u8], w: u16, h: u16, n: u32, cursor: (u16, u16)) -> [Rect; 2] {
    const S: u32 = 256;
    let (w32, h32) = (u32::from(w), u32::from(h));
    let bounce = |t: u32, span: u32| {
        if span == 0 {
            0
        } else {
            let p = t % (2 * span);
            if p < span { p } else { 2 * span - p }
        }
    };
    let bx = bounce(n * 7, w32.saturating_sub(S));
    let by = bounce(n * 5, h32.saturating_sub(S));
    let stride = usize::from(w) * 4;
    // Clear a margin around the box so its previous position is erased.
    let (cx, cy) = (bx.saturating_sub(8), by.saturating_sub(8));
    let (cw, ch) = ((S + 16).min(w32 - cx), (S + 16).min(h32 - cy));
    for y in cy..cy + ch {
        for x in cx..cx + cw {
            let i = y as usize * stride + x as usize * 4;
            let inside = x >= bx && x < bx + S && y >= by && y < by + S;
            let c = if inside {
                [((x - bx) as u8), ((y - by) as u8), (n & 0xff) as u8, 0xff]
            } else {
                [24, 24, 32, 0xff]
            };
            px[i..i + 4].copy_from_slice(&c);
        }
    }
    let (mx, my) = (
        u32::from(cursor.0).min(w32 - 1),
        u32::from(cursor.1).min(h32 - 1),
    );
    for d in 0..9u32 {
        for (x, y) in [
            (mx.saturating_sub(4) + d, my),
            (mx, my.saturating_sub(4) + d),
        ] {
            if x < w32 && y < h32 {
                let i = y as usize * stride + x as usize * 4;
                px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    [
        Rect::new(cx as u16, cy as u16, cw as u16, ch as u16),
        Rect::new(
            mx.saturating_sub(4) as u16,
            my.saturating_sub(4) as u16,
            9,
            9,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use ela_session::{Frontend, FrontendEvent, ProtocolRegistry, SessionManager, Size, wire};

    use super::*;

    #[derive(Default)]
    struct Meter {
        messages: AtomicU64,
        update_bytes: AtomicU64,
        latest_stats: std::sync::Mutex<Option<ela_session::SessionStats>>,
        full_surface: std::sync::Mutex<(u64, u64)>,
    }

    impl Frontend for Meter {
        fn event(&self, event: FrontendEvent) {
            if let FrontendEvent::Stats { stats } = event {
                *self.latest_stats.lock().unwrap() = Some(stats);
            }
        }
        fn data(&self, message: Vec<u8>) {
            let frame = wire::parse_frame(&message).expect("frame message");
            self.messages.fetch_add(1, Ordering::SeqCst);
            // The opening frame (JPEG bands) and its lossless pass cover the surface; the
            // rest is the box.
            if frame
                .rects
                .iter()
                .any(|r| r.encoding == wire::Encoding::Jpeg)
            {
                self.full_surface.lock().unwrap().0 += 1;
                return;
            }
            if let [only] = frame.rects.as_slice()
                && only.rect == Rect::new(0, 0, 1920, 1080)
            {
                self.full_surface.lock().unwrap().1 += 1;
                return;
            }
            assert!(
                frame.rects.len() <= 2,
                "box and crosshair should be all that changes"
            );
            self.update_bytes
                .fetch_add(message.len() as u64, Ordering::SeqCst);
        }
    }

    fn bench_config(options: serde_json::Value) -> ResolvedConfig {
        ResolvedConfig {
            connection: None,
            host: "bench".into(),
            port: 0,
            username: None,
            password: None,
            options: options.as_object().unwrap().clone(),
            initial_size: Size {
                width: 1920,
                height: 1080,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }

    fn mark_in(px: &[u8]) -> u8 {
        let row = 16 * 1920 * 4;
        (0..8).fold(0, |value, bit| {
            let x = 16 + bit * 32;
            value | (u8::from(px[row + x * 4] > 128) << bit)
        })
    }

    #[tokio::test]
    async fn the_mark_counts_moves_and_key_presses_only() {
        let mut registry = ProtocolRegistry::new();
        registry.register(BenchFramebuffer);
        let mgr = SessionManager::new(registry);
        let config = bench_config(json!({ "mode": "idle", "echo": true }));
        let id = mgr
            .open("bench-fb", config, Arc::new(Meter::default()))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let key = |down| SessionCommand::Key {
            code: "KeyQ".into(),
            down,
        };
        for command in [
            SessionCommand::MouseMove { x: 5, y: 5 },
            key(true),
            key(false),
        ] {
            mgr.send(id, command).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (_, _, px) = mgr.snapshot(id).expect("a surface");
        assert_eq!(mark_in(&px), 2);
        mgr.close(id);
    }

    #[derive(Default)]
    struct Terminal {
        bytes: std::sync::Mutex<Vec<u8>>,
        messages: AtomicU64,
    }

    impl Frontend for Terminal {
        fn event(&self, _: FrontendEvent) {}
        fn data(&self, message: Vec<u8>) {
            self.bytes.lock().unwrap().extend_from_slice(&message[1..]);
            self.messages.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn an_echo_comes_through_a_flood_and_ctrl_c_ends_the_flood() {
        let mut registry = ProtocolRegistry::new();
        registry.register(BenchTerminal);
        let mgr = SessionManager::new(registry);
        let terminal = Arc::new(Terminal::default());
        let config = bench_config(json!({ "echo": "flood" }));
        let id = mgr.open("bench-term", config, terminal.clone()).unwrap();

        let pump = |until: Duration| {
            let (mgr, terminal) = (mgr.clone(), terminal.clone());
            async move {
                let (start, mut acked) = (Instant::now(), 0);
                while start.elapsed() < until {
                    let received = terminal.messages.load(Ordering::SeqCst);
                    for _ in acked..received {
                        mgr.ack(id);
                    }
                    acked = received;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        };
        pump(Duration::from_millis(100)).await;
        mgr.send(
            id,
            SessionCommand::Text {
                data: "MARK\x03".into(),
            },
        )
        .unwrap();
        pump(Duration::from_millis(300)).await;

        let after_stop = terminal.bytes.lock().unwrap().len();
        pump(Duration::from_millis(100)).await;
        let bytes = terminal.bytes.lock().unwrap();
        assert_eq!(bytes.len(), after_stop, "the flood kept going");
        assert!(bytes.ends_with(b"MARK\x03") || bytes.windows(5).any(|w| w == b"MARK\x03"));
        mgr.close(id);
    }

    #[tokio::test]
    async fn moving_box_sends_only_what_changed() {
        let mut registry = ProtocolRegistry::new();
        registry.register(BenchFramebuffer);
        let mgr = SessionManager::new(registry);
        let meter = Arc::new(Meter::default());
        let mut options = serde_json::Map::new();
        options.insert("mode".into(), json!("partial"));
        let config = ResolvedConfig {
            connection: None,
            host: "bench".into(),
            port: 0,
            username: None,
            password: None,
            options,
            initial_size: Size {
                width: 1920,
                height: 1080,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        };
        let id = mgr.open("bench-fb", config, meter.clone()).unwrap();
        // Far from where the box starts, so a bounding box would span most of the surface.
        mgr.send(id, SessionCommand::MouseMove { x: 1900, y: 1060 })
            .unwrap();

        let start = Instant::now();
        let mut acked = 0;
        while start.elapsed() < Duration::from_millis(1200) {
            let received = meter.messages.load(Ordering::SeqCst);
            for _ in acked..received {
                mgr.ack(id);
            }
            acked = received;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        mgr.close(id);

        let bytes = meter.update_bytes.load(Ordering::SeqCst);
        let updates = meter.messages.load(Ordering::SeqCst);
        let mb_per_s = bytes as f64 / 1_000_000.0 / start.elapsed().as_secs_f64();
        eprintln!("{updates} updates, {mb_per_s:.1} MB/s");
        assert!(updates > 10, "too few updates to judge: {updates}");
        assert!(mb_per_s < 30.0, "{mb_per_s:.1} MB/s");

        let stats = meter
            .latest_stats
            .lock()
            .unwrap()
            .expect("stats arrive every second");
        assert!(
            stats.take_ns > 0 && stats.encode_ns > 0 && stats.send_ns > 0 && stats.ack_wait_ns > 0,
            "{stats:?}"
        );
        // The opening frame goes lossy, then is sent again losslessly despite the moving box.
        assert_eq!(*meter.full_surface.lock().unwrap(), (1, 1));
    }
}
