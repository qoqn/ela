use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use serde::Serialize;
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::frame::TakenFrame;
use crate::session::{
    Answer, PromptId, Prompts, SavedPasswords, SessionCommand, SessionIo, SessionSink,
    SessionStatus,
};
use crate::{
    FrameStore, ProtocolRegistry, RemoteFiles, RendererKind, ResolvedConfig, SessionError, encode,
    wire,
};

pub type SessionId = Uuid;

const TERMINAL_CREDITS: usize = 8;
const TERMINAL_MAX_MESSAGE: usize = 256 * 1024;
/// Framebuffer messages in flight: one, so the newest frame always wins.
const FRAME_CREDITS: usize = 1;
const REFINE_DELAY: Duration = Duration::from_millis(200);

pub trait Frontend: Send + Sync + 'static {
    fn event(&self, event: FrontendEvent);
    /// Output stops until the UI calls [`SessionManager::ack`]: credits are the backpressure.
    fn data(&self, message: Vec<u8>);
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrontendEvent {
    Status { status: SessionStatus },
    Stats { stats: SessionStats },
    ClipboardWrite { text: String },
    ClipboardRead,
    Cursor { cursor: RemoteCursor },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum RemoteCursor {
    Default,
    Hidden,
    Image {
        width: u16,
        height: u16,
        hot_x: u16,
        hot_y: u16,
        rgba: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, specta::Type)]
pub struct SessionStats {
    pub produced: u64,
    pub sent: u64,
    pub bytes: u64,
    pub take_ns: u64,
    pub encode_ns: u64,
    pub send_ns: u64,
    pub credit_wait_ns: u64,
}

#[derive(Default)]
struct Counters {
    produced: AtomicU64,
    sent: AtomicU64,
    bytes: AtomicU64,
    take_ns: AtomicU64,
    encode_ns: AtomicU64,
    send_ns: AtomicU64,
    credit_wait_ns: AtomicU64,
}

struct Handle {
    commands: mpsc::Sender<SessionCommand>,
    credits: Arc<Semaphore>,
    max_credits: usize,
    cancel: CancellationToken,
    prompts: Prompts,
    frame: Arc<FrameStore>,
    files: Arc<Mutex<Option<Arc<dyn RemoteFiles>>>>,
}

pub struct SessionManager {
    registry: ProtocolRegistry,
    sessions: Mutex<HashMap<SessionId, Handle>>,
    passwords: Option<Arc<dyn SavedPasswords>>,
}

/// Finishes a Session's configuration once it has a Tab to ask on: reading saved Secrets may
/// need the Vault unlocked.
#[async_trait::async_trait]
pub trait Resolve: Send + 'static {
    async fn resolve(self: Box<Self>, sink: &SessionSink) -> Result<ResolvedConfig, SessionError>;
}

#[async_trait::async_trait]
impl Resolve for ResolvedConfig {
    async fn resolve(self: Box<Self>, _: &SessionSink) -> Result<ResolvedConfig, SessionError> {
        Ok(*self)
    }
}

impl SessionManager {
    pub fn new(registry: ProtocolRegistry) -> Arc<Self> {
        Self::keeping(registry, None)
    }

    pub fn keeping(
        registry: ProtocolRegistry,
        passwords: Option<Arc<dyn SavedPasswords>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            sessions: Mutex::new(HashMap::new()),
            passwords,
        })
    }

    pub fn registry(&self) -> &ProtocolRegistry {
        &self.registry
    }

    /// Must be called inside a Tokio runtime.
    pub fn open(
        self: &Arc<Self>,
        protocol_id: &str,
        config: ResolvedConfig,
        frontend: Arc<dyn Frontend>,
    ) -> Result<SessionId, SessionError> {
        self.open_with(protocol_id, config, frontend)
    }

    /// The configuration is finished inside the Session, so its prompts reach the Tab.
    pub fn open_with(
        self: &Arc<Self>,
        protocol_id: &str,
        resolve: impl Resolve,
        frontend: Arc<dyn Frontend>,
    ) -> Result<SessionId, SessionError> {
        let protocol = self
            .registry
            .get(protocol_id)
            .ok_or_else(|| SessionError::UnknownProtocol(protocol_id.to_owned()))?;
        let renderer = protocol.renderer();
        let id = Uuid::new_v4();

        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let (term_tx, term_rx) = mpsc::channel::<Bytes>(64);
        let frame = Arc::new(FrameStore::new());
        let cancel = CancellationToken::new();
        let max_credits = if renderer == RendererKind::Framebuffer {
            FRAME_CREDITS
        } else {
            TERMINAL_CREDITS
        };
        let credits = Arc::new(Semaphore::new(max_credits));
        let counters = Arc::new(Counters::default());
        let prompts = Prompts::default();
        let files = Arc::new(Mutex::new(None));

        self.sessions.lock().unwrap().insert(
            id,
            Handle {
                commands: cmd_tx,
                credits: credits.clone(),
                max_credits,
                cancel: cancel.clone(),
                prompts: prompts.clone(),
                frame: frame.clone(),
                files: files.clone(),
            },
        );

        let fe = frontend.clone();
        tokio::spawn(async move {
            while let Some(event) = events_rx.recv().await {
                let closed = matches!(
                    event,
                    FrontendEvent::Status {
                        status: SessionStatus::Closed { .. }
                    }
                );
                fe.event(event);
                if closed {
                    break;
                }
            }
        });

        let pump_cancel = cancel.child_token();
        let (lossy_tx, lossy_rx) = tokio::sync::oneshot::channel();
        match renderer {
            RendererKind::Framebuffer => {
                drop(term_rx);
                tokio::spawn(pump_frames(
                    frame.clone(),
                    frontend.clone(),
                    credits,
                    counters.clone(),
                    lossy_rx,
                    pump_cancel.clone(),
                ));
            }
            _ => {
                tokio::spawn(pump_terminal(
                    term_rx,
                    frontend.clone(),
                    credits,
                    counters.clone(),
                    pump_cancel.clone(),
                ));
            }
        }

        let fe = frontend.clone();
        let ticker_frame = frame.clone();
        let ticker_cancel = pump_cancel.clone();
        let is_frame = renderer == RendererKind::Framebuffer;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    _ = ticker_cancel.cancelled() => break,
                }
                let produced = if is_frame {
                    ticker_frame.produced()
                } else {
                    counters.produced.load(Ordering::Relaxed)
                };
                fe.event(FrontendEvent::Stats {
                    stats: SessionStats {
                        produced,
                        sent: counters.sent.load(Ordering::Relaxed),
                        bytes: counters.bytes.load(Ordering::Relaxed),
                        take_ns: counters.take_ns.load(Ordering::Relaxed),
                        encode_ns: counters.encode_ns.load(Ordering::Relaxed),
                        send_ns: counters.send_ns.load(Ordering::Relaxed),
                        credit_wait_ns: counters.credit_wait_ns.load(Ordering::Relaxed),
                    },
                });
            }
        });

        let sink = SessionSink {
            events: events_tx,
            terminal: term_tx,
            frame,
            prompts,
            passwords: self.passwords.clone(),
        };
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            sink.status(SessionStatus::Connecting { jump_host: None });
            let resolve: Box<dyn Resolve> = Box::new(resolve);
            let run = async {
                let config = resolve.resolve(&sink).await?;
                let _ = lossy_tx.send(config.option_str("image_quality") != Some("lossless"));
                let session = protocol.connect(config, &sink).await?;
                *files.lock().unwrap() = session.files();
                sink.status(SessionStatus::Connected);
                session
                    .run(SessionIo {
                        commands: cmd_rx,
                        sink: sink.clone(),
                        cancel: cancel.clone(),
                    })
                    .await
            };
            let result = tokio::select! {
                r = run => r,
                _ = cancel.cancelled() => Err(SessionError::Cancelled),
            };
            let error = match result {
                Ok(()) | Err(SessionError::Cancelled) => None,
                Err(e) => {
                    tracing::warn!(session = %id, error = %e, "session ended with error");
                    Some(e)
                }
            };
            // Give the pump a moment to flush the last output before closing.
            tokio::time::sleep(Duration::from_millis(100)).await;
            sink.status(SessionStatus::Closed { error });
            manager.sessions.lock().unwrap().remove(&id);
            cancel.cancel();
        });

        Ok(id)
    }

    pub fn send(&self, id: SessionId, command: SessionCommand) -> Result<(), SessionError> {
        let sessions = self.sessions.lock().unwrap();
        let handle = sessions.get(&id).ok_or(SessionError::UnknownSession)?;
        if let Err(e) = handle.commands.try_send(command) {
            tracing::debug!(session = %id, "input dropped: {e}");
        }
        Ok(())
    }

    /// An answer to a prompt that is no longer open, or of another kind, is dropped.
    pub fn answer(
        &self,
        id: SessionId,
        prompt: PromptId,
        answer: Answer,
    ) -> Result<(), SessionError> {
        let sessions = self.sessions.lock().unwrap();
        let handle = sessions.get(&id).ok_or(SessionError::UnknownSession)?;
        if !handle.prompts.answer(prompt, answer) {
            tracing::debug!(session = %id, prompt, "answer to a prompt that isn't open dropped");
        }
        Ok(())
    }

    pub fn ack(&self, id: SessionId) {
        if let Some(handle) = self.sessions.lock().unwrap().get(&id)
            && handle.credits.available_permits() < handle.max_credits
        {
            handle.credits.add_permits(1);
        }
    }

    pub fn snapshot(&self, id: SessionId) -> Option<(u16, u16, Vec<u8>)> {
        let frame = self.sessions.lock().unwrap().get(&id)?.frame.clone();
        Some(frame.snapshot()).filter(|(width, height, _)| *width > 0 && *height > 0)
    }

    pub fn files(&self, id: SessionId) -> Option<Arc<dyn RemoteFiles>> {
        self.sessions
            .lock()
            .unwrap()
            .get(&id)?
            .files
            .lock()
            .unwrap()
            .clone()
    }

    pub fn close(&self, id: SessionId) {
        if let Some(handle) = self.sessions.lock().unwrap().get(&id) {
            handle.cancel.cancel();
        }
    }

    pub fn close_all(&self) {
        for handle in self.sessions.lock().unwrap().values() {
            handle.cancel.cancel();
        }
    }
}

async fn acquire_credit(credits: &Semaphore, cancel: &CancellationToken) -> bool {
    tokio::select! {
        permit = credits.acquire() => match permit {
            Ok(p) => { p.forget(); true }
            Err(_) => false,
        },
        _ = cancel.cancelled() => false,
    }
}

/// Terminal output: never dropped. Whatever piled up while waiting for a credit is
/// coalesced into one message, so a flood costs few IPC round-trips while a single
/// keystroke echo goes out immediately.
async fn pump_terminal(
    mut rx: mpsc::Receiver<Bytes>,
    frontend: Arc<dyn Frontend>,
    credits: Arc<Semaphore>,
    counters: Arc<Counters>,
    cancel: CancellationToken,
) {
    loop {
        if !acquire_credit(&credits, &cancel).await {
            return;
        }
        let first = tokio::select! {
            m = rx.recv() => match m { Some(b) => b, None => return },
            _ = cancel.cancelled() => return,
        };
        let mut msg = Vec::with_capacity(1 + first.len());
        msg.push(wire::MSG_TERMINAL);
        msg.extend_from_slice(&first);
        let mut chunks = 1;
        while msg.len() < TERMINAL_MAX_MESSAGE {
            match rx.try_recv() {
                Ok(b) => {
                    msg.extend_from_slice(&b);
                    chunks += 1;
                }
                Err(_) => break,
            }
        }
        counters.produced.fetch_add(chunks, Ordering::Relaxed);
        counters.sent.fetch_add(1, Ordering::Relaxed);
        counters
            .bytes
            .fetch_add(msg.len() as u64, Ordering::Relaxed);
        frontend.data(msg);
    }
}

async fn pump_frames(
    frame: Arc<FrameStore>,
    frontend: Arc<dyn Frontend>,
    credits: Arc<Semaphore>,
    counters: Arc<Counters>,
    allow_lossy: tokio::sync::oneshot::Receiver<bool>,
    cancel: CancellationToken,
) {
    // Nothing is drawn before the Session is configured.
    let allow_lossy = tokio::select! {
        allowed = allow_lossy => allowed.unwrap_or(true),
        _ = cancel.cancelled() => return,
    };
    let add_time = |counter: &AtomicU64, since: Instant| {
        counter.fetch_add(since.elapsed().as_nanos() as u64, Ordering::Relaxed);
    };
    let mut refine_at: Option<tokio::time::Instant> = None;
    loop {
        tokio::select! {
            _ = frame.changed() => {}
            _ = tokio::time::sleep_until(refine_at.unwrap_or_else(tokio::time::Instant::now)), if refine_at.is_some() => {
                // The next band follows once this one is painted; a new lossy update puts it off.
                refine_at = frame.queue_refine().then(tokio::time::Instant::now);
            }
            _ = cancel.cancelled() => return,
        }
        let changed = Instant::now();
        if !acquire_credit(&credits, &cancel).await {
            return;
        }
        add_time(&counters.credit_wait_ns, changed);
        let taking = Instant::now();
        let taken = frame.take_message(allow_lossy);
        add_time(&counters.take_ns, taking);
        match taken {
            Some(TakenFrame { message, lossy }) => {
                let msg = if lossy.is_empty() {
                    message
                } else {
                    // Outside the surface lock, and off the async workers: this takes a while.
                    let encoding = Instant::now();
                    let msg = match tokio::task::spawn_blocking(move || {
                        encode::encode_lossy(&message, &lossy)
                    })
                    .await
                    {
                        Ok(msg) => msg,
                        Err(_) => return,
                    };
                    add_time(&counters.encode_ns, encoding);
                    refine_at = Some(tokio::time::Instant::now() + REFINE_DELAY);
                    msg
                };
                counters.sent.fetch_add(1, Ordering::Relaxed);
                counters
                    .bytes
                    .fetch_add(msg.len() as u64, Ordering::Relaxed);
                let sending = Instant::now();
                frontend.data(msg);
                add_time(&counters.send_ns, sending);
            }
            None => credits.add_permits(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{PassphraseAnswer, PasswordAnswer, Prompt, Tried};
    use crate::{Protocol, Session, Size};
    use async_trait::async_trait;

    struct Echo;
    struct EchoSession;

    #[async_trait]
    impl Protocol for Echo {
        fn id(&self) -> &'static str {
            "echo"
        }
        fn display_name(&self) -> &'static str {
            "Echo"
        }
        fn default_port(&self) -> Option<u16> {
            None
        }
        fn renderer(&self) -> RendererKind {
            RendererKind::Terminal
        }
        async fn connect(
            &self,
            _: ResolvedConfig,
            _: &SessionSink,
        ) -> Result<Box<dyn Session>, SessionError> {
            Ok(Box::new(EchoSession))
        }
    }

    #[async_trait]
    impl Session for EchoSession {
        async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
            while let Some(cmd) = io.commands.recv().await {
                if let SessionCommand::Text { data } = cmd {
                    io.sink.terminal(data.into_bytes()).await?;
                }
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct Recorder {
        data: Mutex<Vec<Vec<u8>>>,
        events: Mutex<Vec<FrontendEvent>>,
    }

    impl Frontend for Recorder {
        fn event(&self, event: FrontendEvent) {
            self.events.lock().unwrap().push(event)
        }
        fn data(&self, message: Vec<u8>) {
            self.data.lock().unwrap().push(message)
        }
    }

    fn config() -> ResolvedConfig {
        ResolvedConfig {
            connection: None,
            host: "x".into(),
            port: 0,
            username: None,
            password: None,
            options: Default::default(),
            initial_size: Size {
                width: 80,
                height: 24,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }

    #[tokio::test]
    async fn terminal_round_trip_respects_credits() {
        let mut reg = ProtocolRegistry::new();
        reg.register(Echo);
        let mgr = SessionManager::new(reg);
        let rec = Arc::new(Recorder::default());
        let id = mgr.open("echo", config(), rec.clone()).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;

        for i in 0..20 {
            mgr.send(
                id,
                SessionCommand::Text {
                    data: format!("{i};"),
                },
            )
            .unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rec.data.lock().unwrap().len() <= TERMINAL_CREDITS);
        for _ in 0..40 {
            mgr.ack(id);
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let all: Vec<u8> = rec
            .data
            .lock()
            .unwrap()
            .iter()
            .flat_map(|m| m[1..].to_vec())
            .collect();
        let expected: String = (0..20).map(|i| format!("{i};")).collect();
        assert_eq!(String::from_utf8(all).unwrap(), expected);

        mgr.close(id);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(rec.events.lock().unwrap().iter().any(|e| matches!(
            e,
            FrontendEvent::Status {
                status: SessionStatus::Closed { error: None }
            }
        )));
    }

    type Script = Arc<
        dyn Fn(
                SessionSink,
            )
                -> std::pin::Pin<Box<dyn Future<Output = Result<(), SessionError>> + Send>>
            + Send
            + Sync,
    >;

    struct Scripted(Script);

    #[async_trait]
    impl Protocol for Scripted {
        fn id(&self) -> &'static str {
            "scripted"
        }
        fn display_name(&self) -> &'static str {
            "Scripted"
        }
        fn default_port(&self) -> Option<u16> {
            None
        }
        fn renderer(&self) -> RendererKind {
            RendererKind::Terminal
        }
        async fn connect(
            &self,
            _: ResolvedConfig,
            sink: &SessionSink,
        ) -> Result<Box<dyn Session>, SessionError> {
            (self.0)(sink.clone()).await?;
            Ok(Box::new(EchoSession))
        }
    }

    fn scripted<F, Fut>(script: F) -> (Arc<SessionManager>, Arc<Recorder>, SessionId)
    where
        F: Fn(SessionSink) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), SessionError>> + Send + 'static,
    {
        let mut reg = ProtocolRegistry::new();
        reg.register(Scripted(Arc::new(move |sink| Box::pin(script(sink)))));
        let mgr = SessionManager::new(reg);
        let rec = Arc::new(Recorder::default());
        let id = mgr.open("scripted", config(), rec.clone()).unwrap();
        (mgr, rec, id)
    }

    fn saw(rec: &Recorder, check: impl Fn(&SessionStatus) -> bool) -> bool {
        rec.events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, FrontendEvent::Status { status } if check(status)))
    }

    fn prompts(rec: &Recorder) -> Vec<(PromptId, Option<String>)> {
        rec.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                FrontendEvent::Status {
                    status: SessionStatus::Asking { id, jump_host, .. },
                } => Some((*id, jump_host.clone())),
                _ => None,
            })
            .collect()
    }

    async fn until_asked(rec: &Recorder, count: usize) -> PromptId {
        for _ in 0..100 {
            if let Some((id, _)) = prompts(rec).get(count - 1) {
                return *id;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("prompt {count} never came");
    }

    fn trust() -> Prompt {
        Prompt::Trust {
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:new".into(),
            known: None,
            known_types: Vec::new(),
        }
    }

    fn password(host: &str) -> Prompt {
        Prompt::Password {
            username: "ops".into(),
            host: host.into(),
            port: 22,
            note: None,
            can_remember: true,
            tried: vec![Tried::Agent],
        }
    }

    fn typed(text: &str) -> Answer {
        Answer::Password(PasswordAnswer {
            password: text.to_owned().into(),
            remember: false,
        })
    }

    fn trusting() -> (Arc<SessionManager>, Arc<Recorder>, SessionId) {
        scripted(|sink| async move {
            match sink.ask_trust(None, trust()).await {
                true => Ok(()),
                false => Err(SessionError::Cancelled),
            }
        })
    }

    #[tokio::test]
    async fn trusting_lets_the_connection_continue() {
        let (mgr, rec, id) = trusting();
        let prompt = until_asked(&rec, 1).await;
        assert!(!saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.answer(id, prompt, Answer::Trust(true)).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.close(id);
    }

    #[tokio::test]
    async fn declining_ends_the_session() {
        let (mgr, rec, id) = trusting();
        let prompt = until_asked(&rec, 1).await;
        mgr.answer(id, prompt, Answer::Trust(false)).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        assert!(saw(&rec, |s| matches!(
            s,
            SessionStatus::Closed { error: None }
        )));
    }

    #[tokio::test]
    async fn closing_ends_the_wait_for_an_answer() {
        let (mgr, rec, id) = trusting();
        until_asked(&rec, 1).await;
        mgr.close(id);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(saw(&rec, |s| matches!(
            s,
            SessionStatus::Closed { error: None }
        )));
    }

    #[tokio::test]
    async fn an_answer_of_another_kind_or_to_another_prompt_is_dropped() {
        let (mgr, rec, id) = trusting();
        let prompt = until_asked(&rec, 1).await;
        mgr.answer(id, prompt, typed("secret")).unwrap();
        mgr.answer(id, prompt + 1, Answer::Trust(true)).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.answer(id, prompt, Answer::Trust(true)).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.close(id);
    }

    #[tokio::test]
    async fn a_doubled_password_for_a_jump_host_never_reaches_the_target() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let kept = received.clone();
        let (mgr, rec, id) = scripted(move |sink| {
            let kept = kept.clone();
            async move {
                for (jump_host, host) in [(Some("gate".to_owned()), "gate.lan"), (None, "db.lan")] {
                    let answer = sink.ask_password(jump_host, password(host)).await;
                    let answer = answer.ok_or(SessionError::Cancelled)?;
                    let text = secrecy::ExposeSecret::expose_secret(&answer.password).to_owned();
                    kept.lock().unwrap().push(text);
                }
                Ok(())
            }
        });
        let gate = until_asked(&rec, 1).await;
        mgr.answer(id, gate, typed("gate password")).unwrap();
        let target = until_asked(&rec, 2).await;
        assert_eq!(prompts(&rec)[1].1, None);
        mgr.answer(id, gate, typed("gate password")).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(received.lock().unwrap().len(), 1);

        mgr.answer(id, target, typed("db password")).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(*received.lock().unwrap(), ["gate password", "db password"]);
        mgr.close(id);
    }

    #[tokio::test]
    async fn a_late_trust_does_not_accept_the_next_host() {
        let (mgr, rec, id) = scripted(|sink| async move {
            for jump_host in [Some("gate".to_owned()), None] {
                if !sink.ask_trust(jump_host, trust()).await {
                    return Err(SessionError::Cancelled);
                }
            }
            Ok(())
        });
        let gate = until_asked(&rec, 1).await;
        mgr.answer(id, gate, Answer::Trust(true)).unwrap();
        until_asked(&rec, 2).await;
        mgr.answer(id, gate, Answer::Trust(true)).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.close(id);
    }

    #[tokio::test]
    async fn each_kind_of_answer_reaches_the_waiting_protocol() {
        let (mgr, rec, id) = scripted(|sink| async move {
            let key = ela_core::CredentialRef::for_key("SHA256:key");
            let passphrase = Prompt::Passphrase {
                key: "~/.ssh/id".into(),
                credential: key,
                note: None,
            };
            let PassphraseAnswer::Typed { passphrase, .. } =
                sink.ask_passphrase(None, passphrase).await
            else {
                return Err(SessionError::AuthFailed("no passphrase".into()));
            };
            let questions = Prompt::Questions {
                username: "ops".into(),
                host: "db.lan".into(),
                port: 22,
                name: String::new(),
                instructions: String::new(),
                questions: vec![crate::Question {
                    text: "Verification code: ".into(),
                    echo: false,
                }],
                refused: false,
            };
            let answers = sink.ask_answers(None, questions).await;
            match answers.as_deref() {
                Some([code])
                    if secrecy::ExposeSecret::expose_secret(code) == "123456"
                        && secrecy::ExposeSecret::expose_secret(&passphrase) == "right" =>
                {
                    Ok(())
                }
                _ => Err(SessionError::AuthFailed("no code".into())),
            }
        });
        let first = until_asked(&rec, 1).await;
        let typed = PassphraseAnswer::Typed {
            passphrase: "right".to_owned().into(),
            remember: false,
        };
        mgr.answer(id, first, Answer::Passphrase(typed)).unwrap();
        let second = until_asked(&rec, 2).await;
        mgr.answer(
            id,
            second,
            Answer::Questions(vec!["123456".to_owned().into()]),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(saw(&rec, |s| matches!(s, SessionStatus::Connected)));
        mgr.close(id);
    }

    #[test]
    fn a_prompt_reads_as_the_ui_expects() {
        let status = SessionStatus::Asking {
            id: 3,
            jump_host: Some("gate".into()),
            prompt: password("gate.lan"),
        };
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({ "state": "asking", "id": 3, "jump_host": "gate", "prompt": { "kind": "password", "username": "ops", "host": "gate.lan", "port": 22, "note": null, "can_remember": true, "tried": ["agent"] } })
        );
    }

    #[test]
    fn unknown_protocol_is_rejected() {
        let mgr = SessionManager::new(ProtocolRegistry::new());
        let err = mgr
            .open("nope", config(), Arc::new(Recorder::default()))
            .unwrap_err();
        assert!(matches!(err, SessionError::UnknownProtocol(_)));
    }
}
