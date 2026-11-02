use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ela_session::{
    Answer, Frontend, FrontendEvent, JumpHost, Prompt, ProtocolRegistry, ResolvedConfig,
    SecretString, SessionCommand, SessionError, SessionId, SessionManager, SessionStatus, Size,
    wire,
};

/// The fixture has one desktop Session, so these tests take turns.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const ENTRY: &str = "[127.0.0.1]:3389";
const STALE_PIN: &str = "SHA256:00:11:22:33";

#[derive(Default)]
struct Recorder {
    frames: Mutex<Vec<(u16, u16)>>,
    lit_pixels: Mutex<usize>,
    surface: Mutex<Vec<u8>>,
    statuses: Mutex<Vec<SessionStatus>>,
    clipboard: Mutex<Vec<String>>,
    clipboard_reads: Mutex<usize>,
}

impl Recorder {
    fn prompt_id(&self) -> ela_session::PromptId {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|s| match s {
                SessionStatus::Asking { id, .. } => Some(*id),
                _ => None,
            })
            .expect("Ela asked nothing")
    }
    fn asked(&self) -> Option<(String, Option<String>)> {
        self.statuses.lock().unwrap().iter().find_map(|s| match s {
            SessionStatus::Asking {
                prompt: Prompt::Trust {
                    fingerprint, known, ..
                },
                ..
            } => Some((fingerprint.clone(), known.clone())),
            _ => None,
        })
    }
    fn trust_prompts(&self) -> Vec<Option<String>> {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| match s {
                SessionStatus::Asking {
                    jump_host,
                    prompt: Prompt::Trust { .. },
                    ..
                } => Some(jump_host.clone()),
                _ => None,
            })
            .collect()
    }
    fn saw_connected(&self) -> bool {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .any(|s| matches!(s, SessionStatus::Connected))
    }
    fn closed(&self) -> Option<Option<SessionError>> {
        self.statuses.lock().unwrap().iter().find_map(|s| match s {
            SessionStatus::Closed { error } => Some(error.clone()),
            _ => None,
        })
    }
}

impl Frontend for Recorder {
    fn event(&self, event: FrontendEvent) {
        match event {
            FrontendEvent::Status { status } => self.statuses.lock().unwrap().push(status),
            FrontendEvent::ClipboardWrite { text } => self.clipboard.lock().unwrap().push(text),
            FrontendEvent::ClipboardRead => *self.clipboard_reads.lock().unwrap() += 1,
            FrontendEvent::Stats { .. } => {}
        }
    }
    fn data(&self, message: Vec<u8>) {
        if let Some(frame) = wire::parse_frame(&message) {
            let mut frames = self.frames.lock().unwrap();
            let size = (frame.surface_w, frame.surface_h);
            let stride = usize::from(frame.surface_w) * 4;
            let mut surface = self.surface.lock().unwrap();
            // A new size starts from black, as in the UI, so old content can't pass for a repaint.
            if frames.last() != Some(&size) {
                surface.clear();
                surface.resize(stride * usize::from(frame.surface_h), 0);
            }
            frames.push(size);
            drop(frames);
            // JPEG rects are left out on purpose: the surface only completes through the
            // lossless pass that follows them, so reaching the desktop proves that pass works.
            for wire::FrameRect {
                rect, data: pixels, ..
            } in frame
                .rects
                .into_iter()
                .filter(|r| r.encoding == wire::Encoding::Raw)
            {
                *self.lit_pixels.lock().unwrap() += pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[..3] != [0, 0, 0])
                    .count();
                let row_len = usize::from(rect.w) * 4;
                for (row, src) in pixels.chunks_exact(row_len).enumerate() {
                    let off = (usize::from(rect.y) + row) * stride + usize::from(rect.x) * 4;
                    surface[off..off + row_len].copy_from_slice(src);
                }
            }
        }
    }
}

const WIDTH: usize = 1280;

fn pixel(surface: &[u8], x: usize, y: usize) -> [u8; 3] {
    let i = (y * WIDTH + x) * 4;
    surface
        .get(i..i + 3)
        .map(|p| [p[0], p[1], p[2]])
        .unwrap_or([0; 3])
}

/// The fixture desktop (dev/rdp) has a light XFCE panel along the top, while xrdp's own
/// login screen is dark blue there. Windows the user left open don't cover the panel.
fn logged_in(surface: &[u8]) -> bool {
    pixel(surface, 300, 12).iter().all(|&c| c > 200)
}

fn pin_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    (dir, path)
}

fn manager(pins: &Path) -> Arc<SessionManager> {
    let mut registry = ProtocolRegistry::new();
    registry.register(ela_rdp::RdpProtocol::with_known_hosts(pins.to_owned()));
    SessionManager::new(registry)
}

fn desktop(host: &str) -> ResolvedConfig {
    ResolvedConfig {
        connection: None,
        host: host.into(),
        port: 3389,
        username: Some("ela".into()),
        password: Some(SecretString::from("ela".to_owned())),
        options: Default::default(),
        initial_size: Size {
            width: WIDTH as u32,
            height: 800,
        },
        jump_hosts: Vec::new(),
        saved_password_lost: false,
    }
}

// The SSH fixture's published port, from where the RDP fixture is reached by its name.
fn through_gate(mut target: ResolvedConfig) -> ResolvedConfig {
    target.options.insert("proxy_jump".into(), "gate".into());
    target.jump_hosts.push(JumpHost {
        name: "gate".into(),
        config: ResolvedConfig {
            connection: None,
            host: "127.0.0.1".into(),
            port: 2222,
            options: Default::default(),
            jump_hosts: Vec::new(),
            saved_password_lost: false,
            ..desktop("gate")
        },
    });
    target
}

fn open(mgr: &Arc<SessionManager>) -> (Arc<Recorder>, SessionId) {
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("rdp", desktop("127.0.0.1"), rec.clone()).unwrap();
    (rec, id)
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..300 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting until {what}");
}

async fn close(mgr: &SessionManager, rec: &Recorder, id: SessionId) {
    mgr.close(id);
    until("closed", || rec.closed().is_some()).await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn desktop_frames_arrive() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();

    for _ in 0..300 {
        mgr.ack(id);
        // Updates are merged while unacknowledged (ADR-0007), so a calm desktop
        // yields few messages. Wait until the desktop's top panel has been painted.
        if logged_in(&rec.surface.lock().unwrap()) {
            break;
        }
        if let Some(error) = rec.closed() {
            panic!("closed early: {error:?}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let frames = rec.frames.lock().unwrap().clone();
    assert!(rec.saw_connected());
    // Set ELA_DUMP_FRAME=path.rgba to keep the session open a few seconds and save
    // the composed screen for inspection by eye.
    if let Ok(path) = std::env::var("ELA_DUMP_FRAME") {
        for _ in 0..60 {
            mgr.ack(id);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        std::fs::write(path, &*rec.surface.lock().unwrap()).unwrap();
    }
    let lit = *rec.lit_pixels.lock().unwrap();
    assert!(
        logged_in(&rec.surface.lock().unwrap()),
        "never reached the desktop ({} frames, {lit} non-black pixels); set ELA_DUMP_FRAME to look",
        frames.len()
    );
    eprintln!("{} frame messages, {lit} non-black pixels", frames.len());
    assert_eq!(frames[0], (1280, 800));
    close(&mgr, &rec, id).await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn the_first_connection_asks_and_the_next_is_silent() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);

    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    let (fingerprint, known) = rec.asked().unwrap();
    assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
    assert_eq!(known, None);
    assert!(!pins.exists(), "nothing is saved before the answer");

    // Longer than the connect timeout, and likely longer than the server keeps an idle
    // connection: answering late must still work.
    tokio::time::sleep(Duration::from_secs(22)).await;
    assert!(
        rec.closed().is_none(),
        "closed while waiting: {:?}",
        rec.closed()
    );
    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(
        std::fs::read_to_string(&pins).unwrap(),
        format!("{ENTRY} {fingerprint}\n")
    );
    close(&mgr, &rec, id).await;

    let (again, id) = open(&mgr);
    until("connected again", || again.saw_connected()).await;
    assert!(again.asked().is_none(), "a pinned certificate must not ask");
    close(&mgr, &again, id).await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn cancelling_ends_the_session_and_saves_nothing() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    mgr.answer(id, rec.prompt_id(), Answer::Trust(false))
        .unwrap();
    until("closed", || rec.closed().is_some()).await;
    assert!(matches!(rec.closed(), Some(None)), "{:?}", rec.closed());
    assert!(!rec.saw_connected());
    assert!(!pins.exists());
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn a_changed_certificate_blocks_until_the_user_replaces_it() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let stale = format!("[other]:3389 SHA256:AA\n{ENTRY} {STALE_PIN}\n");
    std::fs::write(&pins, &stale).unwrap();
    let mgr = manager(&pins);

    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    let (_, known) = rec.asked().unwrap();
    assert_eq!(known.as_deref(), Some(STALE_PIN));
    assert!(!rec.saw_connected());
    mgr.answer(id, rec.prompt_id(), Answer::Trust(false))
        .unwrap();
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(
            rec.closed(),
            Some(Some(SessionError::CertificateUntrusted(_)))
        ),
        "{:?}",
        rec.closed()
    );
    assert_eq!(std::fs::read_to_string(&pins).unwrap(), stale);

    let (rec, id) = open(&mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    let (fingerprint, _) = rec.asked().unwrap();
    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("connected", || rec.saw_connected()).await;
    assert_eq!(
        std::fs::read_to_string(&pins).unwrap(),
        format!("[other]:3389 SHA256:AA\n{ENTRY} {fingerprint}\n")
    );
    close(&mgr, &rec, id).await;

    let (again, id) = open(&mgr);
    until("connected again", || again.saw_connected()).await;
    assert!(again.asked().is_none());
    close(&mgr, &again, id).await;
}

async fn connect_to_desktop(mgr: &Arc<SessionManager>) -> (Arc<Recorder>, SessionId) {
    let (rec, id) = open(mgr);
    until("Ela asks", || rec.asked().is_some()).await;
    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("the desktop shows", || {
        mgr.ack(id);
        logged_in(&rec.surface.lock().unwrap())
    })
    .await;
    (rec, id)
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn resizing_changes_the_remote_desktop_and_it_repaints() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = connect_to_desktop(&mgr).await;

    mgr.send(
        id,
        SessionCommand::Resize {
            width: 1024,
            height: 700,
        },
    )
    .unwrap();
    until("the new size arrives", || {
        mgr.ack(id);
        rec.frames.lock().unwrap().last() == Some(&(1024, 700))
    })
    .await;
    assert!(rec.closed().is_none(), "{:?}", rec.closed());

    // Back at the size `logged_in` knows, starting from black, the desktop must repaint.
    mgr.send(
        id,
        SessionCommand::Resize {
            width: WIDTH as u32,
            height: 800,
        },
    )
    .unwrap();
    until("the desktop repaints at the old size", || {
        mgr.ack(id);
        rec.frames.lock().unwrap().last() == Some(&(WIDTH as u16, 800))
            && logged_in(&rec.surface.lock().unwrap())
    })
    .await;
    assert!(rec.closed().is_none(), "{:?}", rec.closed());
    close(&mgr, &rec, id).await;
}

fn in_desktop(script: &str) -> std::process::Output {
    // The running Xorg names the display: a restarted container keeps stale sockets in /tmp.
    let display = "$(pgrep -u ela -a Xorg | grep -oE ' :[0-9]+' | head -1 | tr -d ' ')";
    let script = format!("export DISPLAY={display}; {script}");
    std::process::Command::new("docker")
        .args(["exec", "-u", "ela", "dev-rdp-1", "sh", "-c", &script])
        .output()
        .expect("docker runs")
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn text_copies_both_ways() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = connect_to_desktop(&mgr).await;
    // Let the clipboard channel finish starting.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let copied = in_desktop(
        "printf 'from the desktop é' | xclip -selection clipboard -loops 2 >/dev/null 2>&1 &",
    );
    assert!(copied.status.success(), "{copied:?}");
    until("the remote text arrives", || {
        rec.clipboard
            .lock()
            .unwrap()
            .iter()
            .any(|t| t == "from the desktop é")
    })
    .await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    mgr.send(id, SessionCommand::ClipboardOffer).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let reads_before_paste = *rec.clipboard_reads.lock().unwrap();
    let paste =
        tokio::task::spawn_blocking(|| in_desktop("timeout 10 xclip -selection clipboard -o"));
    until("the remote side asks", || {
        *rec.clipboard_reads.lock().unwrap() > reads_before_paste
    })
    .await;
    mgr.send(
        id,
        SessionCommand::ClipboardText {
            text: Some("from ela é".into()),
        },
    )
    .unwrap();
    let pasted = paste.await.unwrap();
    assert_eq!(
        String::from_utf8_lossy(&pasted.stdout).trim_end_matches('\0'),
        "from ela é",
        "{pasted:?}"
    );

    close(&mgr, &rec, id).await;
}

#[tokio::test]
async fn a_jump_host_without_a_way_through_is_refused_rather_than_skipped() {
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let rec = Arc::new(Recorder::default());
    mgr.open("rdp", through_gate(desktop("rdp")), rec.clone())
        .unwrap();
    until("closed", || rec.closed().is_some()).await;
    assert!(
        matches!(rec.closed(), Some(Some(ref error)) if error.to_string().contains("did not connect directly")),
        "{:?}",
        rec.closed()
    );
    assert!(rec.trust_prompts().is_empty());
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP and SSH fixtures"]
async fn a_desktop_behind_a_jump_host_checks_both_and_shows() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (dir, pins) = pin_file();
    let known_hosts = dir.path().join("ssh_known_hosts");
    let ssh = ela_ssh::SshProtocol::with_known_hosts(known_hosts.clone());
    let mut registry = ProtocolRegistry::new();
    registry.register(
        ela_rdp::RdpProtocol::with_known_hosts(pins.clone()).through_jump_hosts(ssh.transport()),
    );
    let mgr = SessionManager::new(registry);

    let rec = Arc::new(Recorder::default());
    let id = mgr
        .open("rdp", through_gate(desktop("rdp")), rec.clone())
        .unwrap();
    let mut answered = 0;
    until("the desktop shows", || {
        let asked = rec.trust_prompts().len();
        if asked > answered {
            mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
                .unwrap();
            answered = asked;
        }
        if let Some(error) = rec.closed() {
            panic!("closed early: {error:?}");
        }
        mgr.ack(id);
        logged_in(&rec.surface.lock().unwrap())
    })
    .await;
    assert_eq!(rec.trust_prompts(), [Some("gate".into()), None]);
    let ssh_hosts = std::fs::read_to_string(&known_hosts).unwrap();
    assert!(
        ssh_hosts
            .lines()
            .any(|l| l.starts_with("[127.0.0.1]:2222 ")),
        "{ssh_hosts}"
    );
    let pinned = std::fs::read_to_string(&pins).unwrap();
    assert!(pinned.starts_with("[rdp]:3389 SHA256:"), "{pinned}");
    close(&mgr, &rec, id).await;

    let again = Arc::new(Recorder::default());
    let id = mgr
        .open("rdp", through_gate(desktop("rdp")), again.clone())
        .unwrap();
    until("connected again", || again.saw_connected()).await;
    assert!(again.trust_prompts().is_empty(), "known hops must not ask");
    close(&mgr, &again, id).await;
}

#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn without_a_password_the_tab_asks_after_the_certificate() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = open_without_password(&mgr);
    answer_trust_then_password(&mgr, &rec, id, "ela").await;
    until("connected", || rec.saw_connected()).await;
    assert_eq!(password_prompts(&rec).len(), 1);
    close(&mgr, &rec, id).await;
}

// xrdp has no NLA, so it never refuses a password during the handshake: it connects and shows the
// failure on its own login screen. Asking again needs a server with NLA.
#[tokio::test]
#[ignore = "needs the dev/compose.yml RDP fixture"]
async fn without_nla_a_wrong_password_reaches_the_servers_own_login_screen() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let (_dir, pins) = pin_file();
    let mgr = manager(&pins);
    let (rec, id) = open_without_password(&mgr);
    answer_trust_then_password(&mgr, &rec, id, "wrong").await;
    until("connected", || rec.saw_connected()).await;
    assert_eq!(password_prompts(&rec).len(), 1);
    close(&mgr, &rec, id).await;
}

fn open_without_password(mgr: &Arc<SessionManager>) -> (Arc<Recorder>, SessionId) {
    let mut config = desktop("127.0.0.1");
    config.password = None;
    let rec = Arc::new(Recorder::default());
    let id = mgr.open("rdp", config, rec.clone()).unwrap();
    (rec, id)
}

fn password_prompts(rec: &Recorder) -> Vec<ela_session::PromptId> {
    rec.statuses
        .lock()
        .unwrap()
        .iter()
        .filter_map(|status| match status {
            SessionStatus::Asking {
                id,
                prompt: Prompt::Password { .. },
                ..
            } => Some(*id),
            _ => None,
        })
        .collect()
}

async fn answer_trust_then_password(
    mgr: &SessionManager,
    rec: &Recorder,
    id: SessionId,
    password: &str,
) {
    until("Ela asks to trust", || rec.asked().is_some()).await;
    mgr.answer(id, rec.prompt_id(), Answer::Trust(true))
        .unwrap();
    until("Ela asks for the password", || {
        password_prompts(rec).len() == 1
    })
    .await;
    let answer = Answer::Password(ela_session::PasswordAnswer {
        password: SecretString::from(password),
        remember: false,
    });
    mgr.answer(id, password_prompts(rec)[0], answer).unwrap();
}
