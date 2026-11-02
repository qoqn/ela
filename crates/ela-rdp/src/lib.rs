mod clipboard;
mod keymap;
pub mod pinned_certs;
mod redirect;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use clipboard::{ClipboardBridge, ClipboardEvent};
use ela_session::{
    Capabilities, ExposeSecret, PasswordNote, Prompt, Protocol, Rect, RendererKind, ResolvedConfig,
    SecretString, Session, SessionCommand, SessionError, SessionIo, SessionSink, SessionStatus,
    Size,
};
use ela_transport::{
    CONNECT_TIMEOUT, Opened, ROUTE_OPTION, Route, RouteStream, Transport, route_field,
};
use ironrdp_cliprdr::pdu::{ClipboardFormat, ClipboardFormatId, FormatDataResponse};
use ironrdp_cliprdr::{Cliprdr, CliprdrClient, CliprdrSvcMessages};
use ironrdp_connector::connection_activation::{
    ConnectionActivationSequence, ConnectionActivationState,
};
use ironrdp_connector::sspi::generator::NetworkRequest;
use ironrdp_connector::{
    ClientConnector, ClientConnectorState, ConnectionResult, ConnectorError, ConnectorErrorKind,
    ConnectorResult, Credentials, DesktopSize,
};
use ironrdp_core::WriteBuf;
use ironrdp_displaycontrol::client::DisplayControlClient;
use ironrdp_displaycontrol::pdu::MonitorLayoutEntry;
use ironrdp_dvc::DrdynvcClient;
use ironrdp_egfx::client::{GraphicsPipelineClient, GraphicsPipelineHandler};
use ironrdp_graphics::image_processing::PixelFormat;
use ironrdp_input::{Database, MouseButton, MousePosition, Operation, Scancode, WheelRotations};
use ironrdp_pdu::gcc::{ConnectionType, KeyboardType};
use ironrdp_pdu::geometry::{InclusiveRectangle, Rectangle as _};
use ironrdp_pdu::nego::SecurityProtocol;
use ironrdp_pdu::rdp::capability_sets::{MajorPlatformType, RailSupportLevel};
use ironrdp_pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_pdu::rdp::server_redirection::ServerRedirectionPdu;
use ironrdp_pdu::{Action, PduResult};
use ironrdp_session::image::DecodedImage;
use ironrdp_session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput};
use ironrdp_tokio::{FramedWrite as _, NetworkClient, TokioFramed};
use redirect::Redirect;
use serde_json::json;
use tokio::sync::mpsc;

const PASSWORD_TRIES: usize = 3;
const MAX_REDIRECTS: usize = 8;

type TlsStream = ironrdp_tls::TlsStream<RouteStream>;
type TlsFramed = TokioFramed<TlsStream>;

pub const PROTOCOL_ID: &str = "rdp";

#[derive(Default)]
pub struct RdpProtocol {
    known_hosts: Option<PathBuf>,
    jump_hosts: Option<Arc<dyn Transport>>,
}

impl RdpProtocol {
    pub fn with_known_hosts(path: PathBuf) -> Self {
        Self {
            known_hosts: Some(path),
            ..Self::default()
        }
    }

    pub fn through_jump_hosts(mut self, transport: Arc<dyn Transport>) -> Self {
        self.jump_hosts = Some(transport);
        self
    }
}

#[async_trait]
impl Protocol for RdpProtocol {
    fn id(&self) -> &'static str {
        PROTOCOL_ID
    }
    fn display_name(&self) -> &'static str {
        "RDP"
    }
    fn default_port(&self) -> Option<u16> {
        Some(3389)
    }
    fn renderer(&self) -> RendererKind {
        RendererKind::Framebuffer
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resize: true,
            clipboard: true,
            file_transfer: false,
        }
    }
    fn settings_schema(&self) -> serde_json::Value {
        json!({
            "domain": { "type": "string", "label": "Domain", "group": "sign_in" },
            "image_quality": {
                "type": "enum",
                "values": ["auto", "lossless"],
                "labels": { "auto": "Automatic", "lossless": "Lossless" },
                "default": "auto",
                "label": "Image quality",
                "hint": "Automatic sends large changes as JPEG and sharpens them once the screen is still.",
            },
            "clipboard": {
                "type": "enum",
                "values": ["on", "off"],
                "labels": { "on": "On", "off": "Off" },
                "default": "on",
                "label": "Clipboard sync",
            },
            ROUTE_OPTION: route_field(),
        })
    }

    async fn connect(
        &self,
        config: ResolvedConfig,
        sink: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError> {
        let pins = self
            .known_hosts
            .clone()
            .or_else(pinned_certs::default_path)
            .ok_or_else(|| {
                SessionError::protocol(
                    "Can't find the config folder, which holds the accepted certificates.",
                )
            })?;
        let route = ela_transport::route(self.jump_hosts.as_deref(), &config, sink).await?;
        match connect(config, sink, &pins, &route, None).await {
            Ok(session) => Ok(Box::new(session)),
            Err(error) => {
                route.close().await;
                Err(error)
            }
        }
    }
}

/// An open TLS connection whose certificate hasn't been judged yet. No credentials have
/// been sent over it.
struct TlsLink {
    tls: TlsStream,
    server_public_key: Vec<u8>,
    connector: ClientConnector,
    should_upgrade: ironrdp_tokio::ShouldUpgrade,
    clipboard: mpsc::UnboundedReceiver<ClipboardEvent>,
    unvalidated: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Judgement {
    Proceed,
    Ask {
        fingerprint: String,
        known: Option<String>,
    },
}

/// A certificate from a trusted authority needs no pin: it can be renewed without asking,
/// and an interceptor can't get one for this name. Any other must match what the user accepted.
fn judge(unvalidated: Option<String>, known: Option<String>) -> Judgement {
    match unvalidated {
        None => Judgement::Proceed,
        Some(fingerprint) if known.as_deref() == Some(fingerprint.as_str()) => Judgement::Proceed,
        Some(fingerprint) => Judgement::Ask { fingerprint, known },
    }
}

/// A redirect target's certificate needs no pin when the server that redirected there,
/// already trusted, named it. One it didn't name is refused rather than asked about.
fn check_named(unvalidated: Option<&str>, named: &str) -> Result<(), SessionError> {
    match unvalidated {
        Some(fingerprint) if fingerprint != named => Err(SessionError::CertificateUntrusted(
            format!("{fingerprint} is not the certificate {named} the server redirected to"),
        )),
        _ => Ok(()),
    }
}

async fn connect(
    mut config: ResolvedConfig,
    sink: &SessionSink,
    pins: &Path,
    route: &Arc<dyn Route>,
    redirect: Option<&Redirect>,
) -> Result<RdpSession, SessionError> {
    let mut asking = Asking::new(&config);
    loop {
        let link = with_timeout(async {
            let opened = route.open(&config.host, config.port).await?;
            open_tls(&config, opened, redirect).await
        })
        .await?;
        let judgement = match redirect.and_then(|r| r.certificate.as_deref()) {
            Some(named) => {
                check_named(link.unvalidated.as_deref(), named)?;
                Judgement::Proceed
            }
            None => judge(
                link.unvalidated.clone(),
                pinned_certs::saved(pins, &config.host, config.port)?,
            ),
        };
        let (fingerprint, known) = match judgement {
            Judgement::Proceed => {
                if let Some(fingerprint) = link.unvalidated.clone()
                    && redirect.is_none()
                {
                    sink.status(SessionStatus::Verifying {
                        fingerprint,
                        jump_host: None,
                    });
                }
                // A redirect hands over its own sign-in, so only the first server asks.
                if redirect.is_none() && config.password.is_none() {
                    // As with trust: the user may take longer than the server waits.
                    drop(link);
                    config.password = Some(asking.ask(&config, sink).await?);
                    continue;
                }
                let link = match with_timeout(finalize(link, &config, sink, redirect)).await {
                    Ok(link) => link,
                    Err(SessionError::AuthFailed(detail)) if redirect.is_none() => {
                        tracing::debug!(%detail, "the server refused the password");
                        asking.refused()?;
                        config.password = None;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                asking.keep(&config, sink).await;
                return Ok(RdpSession {
                    link,
                    config: without_password(&config),
                    pins: pins.to_owned(),
                    route: route.clone(),
                });
            }
            Judgement::Ask { fingerprint, known } => (fingerprint, known),
        };
        // The user may take longer than the server keeps an idle connection, so this one
        // is dropped. The next round connects afresh and has to match the accepted pin
        // before the password goes anywhere.
        drop(link);
        if !sink
            .ask_trust(
                None,
                Prompt::Trust {
                    algorithm: "certificate".to_owned(),
                    fingerprint: fingerprint.clone(),
                    known: known.clone(),
                    known_types: Vec::new(),
                },
            )
            .await
        {
            return Err(match known {
                Some(known) => SessionError::CertificateUntrusted(format!(
                    "{fingerprint} does not match {known}"
                )),
                None => SessionError::Cancelled,
            });
        }
        sink.status(SessionStatus::Verifying {
            fingerprint: fingerprint.clone(),
            jump_host: None,
        });
        pinned_certs::save(pins, &config.host, config.port, &fingerprint)?;
    }
}

struct Asking {
    note: Option<PasswordNote>,
    typed: usize,
    remember: bool,
}

impl Asking {
    fn new(config: &ResolvedConfig) -> Self {
        Self {
            note: config.saved_password_lost.then_some(PasswordNote::Missing),
            typed: 0,
            remember: false,
        }
    }

    async fn ask(
        &mut self,
        config: &ResolvedConfig,
        sink: &SessionSink,
    ) -> Result<SecretString, SessionError> {
        let prompt = Prompt::Password {
            username: config.username.clone().unwrap_or_default(),
            host: config.host.clone(),
            port: config.port,
            note: self.note,
            can_remember: config.connection.is_some() && sink.can_keep_passwords(),
            tried: Vec::new(),
        };
        let answer = sink
            .ask_password(None, prompt)
            .await
            .ok_or(SessionError::Cancelled)?;
        self.typed += 1;
        self.remember = answer.remember;
        Ok(answer.password)
    }

    // As many as SSH asks, after the saved password, if any, was refused.
    fn refused(&mut self) -> Result<(), SessionError> {
        if self.typed >= PASSWORD_TRIES {
            return Err(SessionError::AuthFailed(
                "The server refused the password three times.".into(),
            ));
        }
        self.note = Some(if self.typed == 0 {
            PasswordNote::SavedWrong
        } else {
            PasswordNote::Wrong
        });
        self.remember = false;
        Ok(())
    }

    async fn keep(&self, config: &ResolvedConfig, sink: &SessionSink) {
        if let (true, Some(connection), Some(password)) =
            (self.remember, config.connection, config.password.clone())
        {
            sink.keep_password(connection, password).await;
        }
    }
}

async fn with_timeout<T>(
    work: impl Future<Output = Result<T, SessionError>>,
) -> Result<T, SessionError> {
    tokio::time::timeout(CONNECT_TIMEOUT, work)
        .await
        .unwrap_or(Err(SessionError::Timeout))
}

async fn open_tls(
    config: &ResolvedConfig,
    Opened { stream, local_addr }: Opened,
    redirect: Option<&Redirect>,
) -> Result<TlsLink, SessionError> {
    let client_addr = local_addr.unwrap_or(SocketAddr::from(([0, 0, 0, 0], 0)));

    let mut framed = TokioFramed::new(stream);
    let (clipboard_events, clipboard) = mpsc::unbounded_channel();
    let display_control = DisplayControlClient::new(|_| Ok(Vec::new()));
    let mut connector = ClientConnector::new(build_config(config, redirect), client_addr)
        .with_static_channel(
            DrdynvcClient::new()
                .with_dynamic_channel(display_control)
                .with_dynamic_channel_factory(ironrdp_egfx::CHANNEL_NAME, || {
                    GraphicsPipelineClient::new(Box::new(CompositedByActiveStage), None)
                }),
        );
    if config.option_str("clipboard") != Some("off") {
        let bridge = ClipboardBridge::new(clipboard_events);
        connector = connector
            .with_static_channel(Cliprdr::<ironrdp_cliprdr::Client>::new(Box::new(bridge)));
    }
    if let Some(redirect) = redirect {
        framed
            .write_all(&redirect.connection_request()?)
            .await
            .map_err(|e| SessionError::from_io(&e))?;
        connector.state = ClientConnectorState::ConnectionInitiationWaitConfirm {
            requested_protocol: SecurityProtocol::RDSTLS,
        };
    }
    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector)
        .await
        .map_err(map_connector_error)?;

    // This callback runs when the certificate fails normal validation. It can't wait for
    // the user, so it notes the certificate and lets the handshake finish; `connect`
    // decides afterwards, before any credentials are sent.
    let unvalidated = Arc::new(Mutex::new(None));
    let noted = unvalidated.clone();
    let callback: ironrdp_tls::CertificateValidationCallback = Arc::new(move |der, _, _| {
        *noted.lock().unwrap() = Some(pinned_certs::fingerprint(der));
        true
    });
    let tcp = framed.into_inner_no_leftover();
    let (tls, cert) =
        ironrdp_tls::upgrade_with_certificate_validation_callback(tcp, &config.host, callback)
            .await
            .map_err(|e| SessionError::CertificateUntrusted(e.to_string()))?;
    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&cert)
        .ok_or_else(|| SessionError::protocol("server certificate has no usable public key"))?
        .to_vec();
    let unvalidated = unvalidated.lock().unwrap().take();
    Ok(TlsLink {
        tls,
        server_public_key,
        connector,
        should_upgrade,
        clipboard,
        unvalidated,
    })
}

async fn finalize(
    link: TlsLink,
    config: &ResolvedConfig,
    sink: &SessionSink,
    redirect: Option<&Redirect>,
) -> Result<Link, SessionError> {
    let TlsLink {
        mut tls,
        server_public_key,
        mut connector,
        should_upgrade,
        clipboard,
        ..
    } = link;
    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);

    match redirect {
        Some(redirect) => redirect.authenticate(&mut tls).await?,
        None => sink.status(SessionStatus::Authenticating { jump_host: None }),
    }
    let mut framed = TokioFramed::new(tls);
    let result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoNetworkClient,
        config.host.clone().into(),
        server_public_key,
        None,
    )
    .await
    .and_then(|outcome| outcome.into_connected())
    .map_err(map_connector_error)?;

    let mut activation_config = build_config(config, redirect);
    activation_config.credentials = Credentials::UsernamePassword {
        username: redirect.map_or_else(
            || config.username.clone().unwrap_or_default(),
            |r| r.username.clone(),
        ),
        password: String::new(),
    };
    Ok(Link {
        framed,
        result,
        clipboard,
        activation_config,
    })
}

fn build_config(config: &ResolvedConfig, redirect: Option<&Redirect>) -> ironrdp_connector::Config {
    let size = desktop_size(config.initial_size.width, config.initial_size.height);
    let mut built = ironrdp_connector::Config {
        credentials: Credentials::UsernamePassword {
            username: config.username.clone().unwrap_or_default(),
            password: config
                .password
                .as_ref()
                .map(|p| p.expose_secret().to_owned())
                .unwrap_or_default(),
        },
        domain: config
            .option_str("domain")
            .filter(|d| !d.is_empty())
            .map(str::to_owned),
        enable_tls: true,
        enable_credssp: true,
        enable_standard_rdp_security: false,
        keyboard_type: KeyboardType::IBM_ENHANCED,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        connection_type: ConnectionType::Lan,
        ime_file_name: String::new(),
        dig_product_id: String::new(),
        desktop_size: size,
        monitor_layout: None,
        bitmap: None,
        client_build: 0,
        client_name: "ela".to_owned(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_owned(),
        platform: if cfg!(windows) {
            MajorPlatformType::WINDOWS
        } else if cfg!(target_os = "macos") {
            MajorPlatformType::MACINTOSH
        } else {
            MajorPlatformType::UNIX
        },
        enable_server_pointer: true,
        pointer_software_rendering: true,
        request_data: None,
        // Send the password in Client Info too, so servers without NLA (xrdp, older
        // Windows) log in directly instead of showing their own login screen.
        autologon: config.password.is_some(),
        enable_audio_playback: false,
        enable_audio_capture: false,
        compression_type: None,
        multitransport_flags: None,
        support_dyn_vc_gfx_protocol: true,
        performance_flags: PerformanceFlags::default(),
        desktop_scale_factor: 0,
        desktop_physical_size: None,
        desktop_orientation: None,
        device_scale_factor: None,
        monitor_extended_layout: None,
        relative_mouse_input: false,
        hardware_id: None,
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        remote_application_mode: false,
        rail_support_level: RailSupportLevel::empty(),
    };
    if let Some(redirect) = redirect {
        built.credentials = Credentials::UsernamePassword {
            username: redirect.username.clone(),
            password: String::new(),
        };
        built.domain = redirect.domain.clone();
        built.autologon = false;
    }
    built
}

/// Clamps to what RDP servers accept; widths must be even.
fn desktop_size(width: u32, height: u32) -> DesktopSize {
    DesktopSize {
        width: (width.clamp(640, 8192) & !1) as u16,
        height: height.clamp(480, 8192) as u16,
    }
}

fn map_connector_error(err: ConnectorError) -> SessionError {
    match err.kind() {
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => {
            SessionError::AuthFailed(err.report().to_string())
        }
        _ => SessionError::protocol(err.report().to_string()),
    }
}

fn session_error(err: ironrdp_session::SessionError) -> SessionError {
    SessionError::protocol(err.report())
}

// Empty because ActiveStage itself paints what the pipeline decodes into the session image.
struct CompositedByActiveStage;

impl GraphicsPipelineHandler for CompositedByActiveStage {}

/// Kerberos (KDC over the network) is not supported yet; NTLM never calls this.
struct NoNetworkClient;

impl NetworkClient for NoNetworkClient {
    async fn send(&mut self, _: &NetworkRequest) -> ConnectorResult<Vec<u8>> {
        Err(ironrdp_connector::general_err!(
            "Kerberos network requests are not supported yet"
        ))
    }
}

struct RdpSession {
    link: Link,
    /// For following a redirect, which needs no password.
    config: ResolvedConfig,
    pins: PathBuf,
    // A redirect goes to the same host and port, so along the same route.
    route: Arc<dyn Route>,
}

struct Link {
    framed: TlsFramed,
    result: ConnectionResult,
    clipboard: mpsc::UnboundedReceiver<ClipboardEvent>,
    /// For server-requested reactivation. Holds no password, which that doesn't need.
    activation_config: ironrdp_connector::Config,
}

fn without_password(config: &ResolvedConfig) -> ResolvedConfig {
    ResolvedConfig {
        connection: None,
        host: config.host.clone(),
        port: config.port,
        username: config.username.clone(),
        password: None,
        options: config.options.clone(),
        initial_size: config.initial_size,
        jump_hosts: Vec::new(),
        saved_password_lost: false,
    }
}

struct Redirected {
    redirect: Redirect,
    width: u16,
    height: u16,
}

#[async_trait]
impl Session for RdpSession {
    async fn run(self: Box<Self>, mut io: SessionIo) -> Result<(), SessionError> {
        let RdpSession {
            link,
            config,
            pins,
            route,
        } = *self;
        let result = follow(link, config, &pins, &route, &mut io).await;
        route.close().await;
        result
    }
}

async fn follow(
    mut link: Link,
    mut config: ResolvedConfig,
    pins: &Path,
    route: &Arc<dyn Route>,
    io: &mut SessionIo,
) -> Result<(), SessionError> {
    let mut followed = 0;
    loop {
        let Some(redirected) = serve(link, io).await? else {
            return Ok(());
        };
        followed += 1;
        if followed > MAX_REDIRECTS {
            return Err(SessionError::protocol(
                "the server kept redirecting the Session",
            ));
        }
        config.initial_size = Size {
            width: redirected.width.into(),
            height: redirected.height.into(),
        };
        link = connect(
            without_password(&config),
            &io.sink,
            pins,
            route,
            Some(&redirected.redirect),
        )
        .await?
        .link;
        io.sink.status(SessionStatus::Connected);
    }
}

async fn serve(link: Link, io: &mut SessionIo) -> Result<Option<Redirected>, SessionError> {
    let Link {
        mut framed,
        result,
        mut clipboard,
        activation_config,
    } = link;
    let (mut width, mut height) = (result.desktop_size.width, result.desktop_size.height);
    let (io_channel_id, user_channel_id) = (result.io_channel_id, result.user_channel_id);
    let (enable_server_pointer, pointer_software_rendering) = (
        result.enable_server_pointer,
        result.pointer_software_rendering,
    );
    let mut image = DecodedImage::new(PixelFormat::RgbA32, width, height);
    // After a redirect the last screen stays until the new desktop paints over it.
    if io.sink.frame().size() != (width, height) {
        io.sink.frame().resize(width, height);
    }

    let mut stage = ActiveStageBuilder {
        static_channels: result.static_channels,
        user_channel_id,
        io_channel_id,
        message_channel_id: result.message_channel_id,
        share_id: result.share_id,
        compression_type: result.compression_type,
        enable_server_pointer,
        pointer_software_rendering,
    }
    .build();
    let mut input = Database::new();

    loop {
        let outputs = tokio::select! {
            pdu = framed.read_pdu() => {
                let (action, payload) = pdu.map_err(|e| SessionError::from_io(&e))?;
                if action == Action::X224
                    && let Some(pdu) = redirect::freerdp_redirection(&payload, io_channel_id)
                {
                    if pdu.is_informational() {
                        continue;
                    }
                    return Ok(Some(Redirected { redirect: Redirect::new(pdu)?, width, height }));
                }
                stage
                    .process(&mut image, action, &payload, framed.last_read_at())
                    .map_err(session_error)?
            }
            Some(event) = clipboard.recv() => {
                let frame = on_clipboard_event(event, &mut stage, &io.sink);
                write_frame(&mut framed, &frame).await?;
                continue;
            }
            cmd = io.commands.recv() => {
                let Some(cmd) = cmd else { break };
                let events = match cmd {
                    SessionCommand::ReleaseAll => input.release_all(),
                    SessionCommand::Resize { width: wanted_width, height: wanted_height } => {
                        let frame = encode_resize(&mut stage, (wanted_width, wanted_height), (width, height));
                        write_frame(&mut framed, &frame).await?;
                        continue;
                    }
                    SessionCommand::ClipboardOffer => {
                        let frame = encode_clipboard(&mut stage, |cliprdr| cliprdr.initiate_copy(&[text_format()]));
                        write_frame(&mut framed, &frame).await?;
                        continue;
                    }
                    SessionCommand::ClipboardText { text } => {
                        let response = text.map_or_else(FormatDataResponse::new_error, |text| {
                            FormatDataResponse::new_unicode_string(&text)
                        });
                        let frame = encode_clipboard(&mut stage, |cliprdr| cliprdr.submit_format_data(response));
                        write_frame(&mut framed, &frame).await?;
                        continue;
                    }
                    other => {
                        let ops = to_operations(other, width, height);
                        if ops.is_empty() { Default::default() } else { input.apply(ops) }
                    }
                };
                stage.process_fastpath_input(&mut image, &events).map_err(session_error)?
            }
            _ = io.cancel.cancelled() => {
                return Err(SessionError::Cancelled);
            }
        };
        // The graphics pipeline resizes with ResetGraphics instead of a reactivation.
        if (image.width(), image.height()) != (width, height) {
            (width, height) = (image.width(), image.height());
            io.sink.frame().resize(width, height);
        }
        match handle_outputs(outputs, &mut framed, &image, &io.sink).await? {
            Flow::Continue => {}
            Flow::Stop => break,
            Flow::Redirect(pdu) => {
                return Ok(Some(Redirected {
                    redirect: Redirect::new(*pdu)?,
                    width,
                    height,
                }));
            }
            Flow::Reactivate => {
                let activated = reactivate(
                    &mut framed,
                    activation_config.clone(),
                    io_channel_id,
                    user_channel_id,
                )
                .await?;
                (width, height) = (activated.desktop_size.width, activated.desktop_size.height);
                image = DecodedImage::new(PixelFormat::RgbA32, width, height);
                io.sink.frame().resize(width, height);
                let accepted = stage.reactivate(
                    io_channel_id,
                    user_channel_id,
                    activated.share_id,
                    enable_server_pointer,
                    pointer_software_rendering,
                    activated.static_channel_chunk_size,
                );
                if !accepted {
                    return Err(SessionError::protocol(
                        "server sent an invalid channel chunk size",
                    ));
                }
            }
        }
    }
    Ok(None)
}

fn text_format() -> ClipboardFormat {
    ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)
}

/// A clipboard problem never ends the Session.
fn encode_clipboard(
    stage: &mut ActiveStage,
    message: impl FnOnce(&mut CliprdrClient) -> PduResult<CliprdrSvcMessages<ironrdp_cliprdr::Client>>,
) -> Vec<u8> {
    let Some(cliprdr) = stage.get_svc_processor_mut::<CliprdrClient>() else {
        return Vec::new();
    };
    let encoded = message(cliprdr)
        .map_err(|e| e.to_string())
        .and_then(|messages| {
            stage
                .process_svc_processor_messages(messages)
                .map_err(|e| e.to_string())
        });
    encoded.unwrap_or_else(|e| {
        tracing::warn!("clipboard message failed: {e}");
        Vec::new()
    })
}

fn on_clipboard_event(
    event: ClipboardEvent,
    stage: &mut ActiveStage,
    sink: &SessionSink,
) -> Vec<u8> {
    match event {
        // Delay-rendered: this only says text may be available, and sends none of it.
        ClipboardEvent::Ready => {
            encode_clipboard(stage, |cliprdr| cliprdr.initiate_copy(&[text_format()]))
        }
        ClipboardEvent::RemoteCopied => encode_clipboard(stage, |cliprdr| {
            cliprdr.initiate_paste(ClipboardFormatId::CF_UNICODETEXT)
        }),
        ClipboardEvent::RemoteWantsText => {
            sink.clipboard_read();
            Vec::new()
        }
        ClipboardEvent::RemoteWantsOther => encode_clipboard(stage, |cliprdr| {
            cliprdr.submit_format_data(FormatDataResponse::new_error())
        }),
        ClipboardEvent::RemoteText(text) => {
            sink.clipboard_write(text);
            Vec::new()
        }
    }
}

/// Empty if nothing needs to change, or if the server doesn't support resizing, in which case the
/// view keeps scaling the old size.
fn encode_resize(stage: &mut ActiveStage, wanted: (u32, u32), current: (u16, u16)) -> Vec<u8> {
    let (width, height) = MonitorLayoutEntry::adjust_display_size(wanted.0, wanted.1);
    if (width, height) == (u32::from(current.0), u32::from(current.1)) {
        return Vec::new();
    }
    match stage.encode_resize(width, height, None, None) {
        Some(Ok(frame)) => frame,
        Some(Err(e)) => {
            tracing::warn!("resize request failed: {e}");
            Vec::new()
        }
        None => Vec::new(),
    }
}

async fn write_frame(framed: &mut TlsFramed, frame: &[u8]) -> Result<(), SessionError> {
    if frame.is_empty() {
        return Ok(());
    }
    framed
        .write_all(frame)
        .await
        .map_err(|e| SessionError::from_io(&e))
}

struct Activated {
    desktop_size: DesktopSize,
    share_id: u32,
    static_channel_chunk_size: usize,
}

async fn reactivate(
    framed: &mut TlsFramed,
    config: ironrdp_connector::Config,
    io_channel_id: u16,
    user_channel_id: u16,
) -> Result<Activated, SessionError> {
    let mut sequence = ConnectionActivationSequence::new(config, io_channel_id, user_channel_id);
    let mut buf = WriteBuf::new();
    loop {
        let written = ironrdp_tokio::single_sequence_step_read(framed, &mut sequence, &mut buf)
            .await
            .map_err(map_connector_error)?;
        if written.size().is_some() {
            write_frame(framed, buf.filled()).await?;
        }
        if let ConnectionActivationState::Finalized {
            desktop_size,
            share_id,
            static_channel_chunk_size,
            ..
        } = sequence.connection_activation_state()
        {
            return Ok(Activated {
                desktop_size,
                share_id,
                static_channel_chunk_size,
            });
        }
    }
}

enum Flow {
    Continue,
    Stop,
    Reactivate,
    Redirect(Box<ServerRedirectionPdu>),
}

async fn handle_outputs(
    outputs: Vec<ActiveStageOutput>,
    framed: &mut TlsFramed,
    image: &DecodedImage,
    sink: &SessionSink,
) -> Result<Flow, SessionError> {
    for out in outputs {
        match out {
            ActiveStageOutput::ResponseFrame(frame) => {
                framed
                    .write_all(&frame)
                    .await
                    .map_err(|e| SessionError::from_io(&e))?;
            }
            ActiveStageOutput::GraphicsUpdate(rect) => publish(image, &rect, sink),
            ActiveStageOutput::Terminate(reason) => {
                tracing::info!(?reason, "RDP session terminated by server");
                return Ok(Flow::Stop);
            }
            ActiveStageOutput::DeactivateAll => return Ok(Flow::Reactivate),
            ActiveStageOutput::ServerRedirection(pdu) => return Ok(Flow::Redirect(pdu)),
            _ => {}
        }
    }
    Ok(Flow::Continue)
}

fn publish(image: &DecodedImage, rect: &InclusiveRectangle, sink: &SessionSink) {
    let r = Rect::new(rect.left, rect.top, rect.width(), rect.height());
    sink.frame().blit(r, image.data(), image.stride());
}

fn to_operations(cmd: SessionCommand, width: u16, height: u16) -> Vec<Operation> {
    let pos = |x: u16, y: u16| MousePosition {
        x: x.min(width - 1),
        y: y.min(height - 1),
    };
    match cmd {
        SessionCommand::Key { code, down } => match keymap::scancode(&code) {
            Some(sc) => {
                let sc = Scancode::from_u16(sc);
                vec![if down {
                    Operation::KeyPressed(sc)
                } else {
                    Operation::KeyReleased(sc)
                }]
            }
            None => vec![],
        },
        SessionCommand::MouseMove { x, y } => vec![Operation::MouseMove(pos(x, y))],
        SessionCommand::MouseButton { button, down, x, y } => {
            let Some(b) = MouseButton::from_web_button(button) else {
                return vec![];
            };
            let press = if down {
                Operation::MouseButtonPressed(b)
            } else {
                Operation::MouseButtonReleased(b)
            };
            vec![Operation::MouseMove(pos(x, y)), press]
        }
        SessionCommand::Wheel {
            vertical,
            horizontal,
        } => {
            let mut ops = Vec::new();
            if vertical != 0 {
                ops.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: true,
                    rotation_units: vertical * 120,
                }));
            }
            if horizontal != 0 {
                ops.push(Operation::WheelRotations(WheelRotations {
                    is_vertical: false,
                    rotation_units: horizontal * 120,
                }));
            }
            ops
        }
        SessionCommand::ReleaseAll
        | SessionCommand::Text { .. }
        | SessionCommand::Resize { .. }
        | SessionCommand::ClipboardOffer
        | SessionCommand::ClipboardText { .. } => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_password_says_why_and_the_third_typed_one_ends_it() {
        let mut asking = Asking {
            note: None,
            typed: 0,
            remember: true,
        };
        asking.refused().unwrap();
        assert_eq!(asking.note, Some(PasswordNote::SavedWrong));
        assert!(!asking.remember);

        asking.typed = PASSWORD_TRIES - 1;
        asking.refused().unwrap();
        assert_eq!(asking.note, Some(PasswordNote::Wrong));

        asking.typed = PASSWORD_TRIES;
        assert!(matches!(asking.refused(), Err(SessionError::AuthFailed(_))));
    }

    fn pin(value: &str) -> Option<String> {
        Some(value.to_owned())
    }

    #[test]
    fn a_redirect_target_must_present_the_certificate_it_was_named_by() {
        assert!(check_named(None, "SHA256:NAMED").is_ok());
        assert!(check_named(Some("SHA256:NAMED"), "SHA256:NAMED").is_ok());
        assert!(matches!(
            check_named(Some("SHA256:OTHER"), "SHA256:NAMED"),
            Err(SessionError::CertificateUntrusted(_))
        ));
    }

    #[test]
    fn a_certificate_from_a_trusted_authority_needs_no_pin() {
        assert_eq!(judge(None, None), Judgement::Proceed);
        assert_eq!(judge(None, pin("SHA256:OLD")), Judgement::Proceed);
    }

    #[test]
    fn a_matching_pin_proceeds() {
        assert_eq!(
            judge(pin("SHA256:AA"), pin("SHA256:AA")),
            Judgement::Proceed
        );
    }

    #[test]
    fn an_unknown_or_different_certificate_asks() {
        assert_eq!(
            judge(pin("SHA256:AA"), None),
            Judgement::Ask {
                fingerprint: "SHA256:AA".into(),
                known: None
            }
        );
        assert_eq!(
            judge(pin("SHA256:AA"), pin("SHA256:OLD")),
            Judgement::Ask {
                fingerprint: "SHA256:AA".into(),
                known: pin("SHA256:OLD")
            }
        );
    }
}
