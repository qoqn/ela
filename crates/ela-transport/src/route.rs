use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use ela_session::{JumpHost, ResolvedConfig, SessionError, SessionSink, SessionStatus};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::{ROUTE_OPTION, has_text, parse_route};

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Stream for T {}

// A named type rather than `Box<dyn Stream>`: ironrdp's futures over a boxed trait object trip
// rustc's higher-ranked `Send` check ("implementation of `Send` is not general enough").
pub struct RouteStream(Box<dyn Stream>);

impl RouteStream {
    pub fn new(stream: impl Stream + 'static) -> Self {
        Self(Box::new(stream))
    }
}

impl AsyncRead for RouteStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for RouteStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.0).poll_shutdown(cx)
    }
}

pub struct Opened {
    pub stream: RouteStream,
    // None through a Jump Host, where this machine's address means nothing to the target.
    pub local_addr: Option<SocketAddr>,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn connect(
        &self,
        jump_hosts: &[JumpHost],
        sink: &SessionSink,
    ) -> Result<Arc<dyn Route>, SessionError>;
}

#[async_trait]
pub trait Route: Send + Sync {
    async fn open(&self, host: &str, port: u16) -> Result<Opened, SessionError>;

    async fn close(&self) {}
}

/// How long reaching a host and signing in may take, for every Protocol, before the user is
/// asked anything.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
// RFC 8305's recommended Connection Attempt Delay.
const ATTEMPT_DELAY: Duration = Duration::from_millis(250);
const KEEPALIVE_IDLE: Duration = Duration::from_secs(30);

pub struct Direct;

#[async_trait]
impl Route for Direct {
    async fn open(&self, host: &str, port: u16) -> Result<Opened, SessionError> {
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, connect_tcp(host, port))
            .await
            .map_err(|_| SessionError::Timeout)?
            .map_err(|e| SessionError::from_io(&e))?;
        let _ = tcp.set_nodelay(true);
        // A NAT or firewall that forgets an idle link would otherwise leave a dead Session open.
        let keepalive = socket2::TcpKeepalive::new()
            .with_time(KEEPALIVE_IDLE)
            .with_interval(KEEPALIVE_IDLE);
        if let Err(error) = socket2::SockRef::from(&tcp).set_tcp_keepalive(&keepalive) {
            tracing::debug!(%error, "couldn't turn on TCP keepalive");
        }
        let local_addr = tcp.local_addr().ok();
        Ok(Opened {
            stream: RouteStream::new(tcp),
            local_addr,
        })
    }
}

async fn connect_tcp(host: &str, port: u16) -> io::Result<TcpStream> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    happy_eyeballs(interleave(addresses)).await
}

/// RFC 8305: the address families take turns, the resolver's first choice first.
fn interleave(addresses: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let Some(first) = addresses.first().copied() else {
        return addresses;
    };
    let (mut same, mut other): (Vec<_>, Vec<_>) = addresses
        .into_iter()
        .partition(|address| address.is_ipv6() == first.is_ipv6());
    same.reverse();
    other.reverse();
    let mut ordered = Vec::with_capacity(same.len() + other.len());
    while let Some(address) = same.pop() {
        ordered.push(address);
        if let Some(address) = other.pop() {
            ordered.push(address);
        }
    }
    ordered.extend(other.into_iter().rev());
    ordered
}

/// A dead address must not hold up the ones after it for the whole OS timeout.
async fn happy_eyeballs(addresses: Vec<SocketAddr>) -> io::Result<TcpStream> {
    let mut waiting = addresses.into_iter();
    let mut attempts = tokio::task::JoinSet::new();
    let mut last_error = None;
    let Some(first) = waiting.next() else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the name has no address",
        ));
    };
    attempts.spawn(TcpStream::connect(first));
    loop {
        let more = waiting.len() > 0;
        tokio::select! {
            finished = attempts.join_next() => match finished {
                Some(Ok(Ok(stream))) => return Ok(stream),
                Some(Ok(Err(error))) => {
                    last_error = Some(error);
                    if let Some(next) = waiting.next() {
                        attempts.spawn(TcpStream::connect(next));
                    } else if attempts.is_empty() {
                        break;
                    }
                }
                Some(Err(joined)) => last_error = Some(io::Error::other(joined)),
                None => break,
            },
            _ = tokio::time::sleep(ATTEMPT_DELAY), if more => {
                if let Some(next) = waiting.next() {
                    attempts.spawn(TcpStream::connect(next));
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no address answered")))
}

pub async fn route(
    transport: Option<&dyn Transport>,
    config: &ResolvedConfig,
    sink: &SessionSink,
) -> Result<Arc<dyn Route>, SessionError> {
    refuse_unsupported_route(config)?;
    if config.jump_hosts.is_empty() {
        return Ok(Arc::new(Direct));
    }
    let transport = transport.ok_or_else(|| {
        SessionError::protocol(
            "This connection goes through a Jump Host, which this build of Ela can't do. It did not connect directly instead.",
        )
    })?;
    let route = transport.connect(&config.jump_hosts, sink).await?;
    sink.status(SessionStatus::Connecting { jump_host: None });
    Ok(route)
}

/// Going direct instead could hand a password to a different machine than the one meant.
pub fn refuse_unsupported_route(config: &ResolvedConfig) -> Result<(), SessionError> {
    if has_text(&config.options, "proxy_command") {
        return Err(SessionError::protocol(
            "This connection goes through a ProxyCommand, which Ela can't do yet. It did not connect directly instead.",
        ));
    }
    let route = parse_route(config.options.get(ROUTE_OPTION)).map_err(SessionError::protocol)?;
    if !route.is_empty() && config.jump_hosts.is_empty() {
        return Err(SessionError::protocol(
            "This connection's Jump Hosts weren't looked up. It did not connect directly instead.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ela_session::Size;
    use serde_json::json;

    use super::*;

    fn config(options: serde_json::Value) -> ResolvedConfig {
        ResolvedConfig {
            connection: None,
            host: "h".into(),
            port: 22,
            username: None,
            password: None,
            options: options.as_object().unwrap().clone(),
            initial_size: Size {
                width: 80,
                height: 24,
            },
            jump_hosts: Vec::new(),
            saved_password_lost: false,
        }
    }

    fn addresses(list: &[&str]) -> Vec<SocketAddr> {
        list.iter().map(|a| a.parse().unwrap()).collect()
    }

    #[test]
    fn address_families_take_turns_starting_with_the_first() {
        let ordered = interleave(addresses(&[
            "[2001:db8::1]:22",
            "[2001:db8::2]:22",
            "[2001:db8::3]:22",
            "192.0.2.1:22",
        ]));
        assert_eq!(
            ordered,
            addresses(&[
                "[2001:db8::1]:22",
                "192.0.2.1:22",
                "[2001:db8::2]:22",
                "[2001:db8::3]:22",
            ])
        );
        let ordered = interleave(addresses(&[
            "192.0.2.1:22",
            "192.0.2.2:22",
            "[2001:db8::1]:22",
        ]));
        assert_eq!(
            ordered,
            addresses(&["192.0.2.1:22", "[2001:db8::1]:22", "192.0.2.2:22"])
        );
    }

    #[tokio::test]
    async fn an_address_that_never_answers_doesnt_hold_up_the_next() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open = listener.local_addr().unwrap();
        // TEST-NET and the IPv6 discard prefix: nothing answers there.
        let silent: Vec<SocketAddr> = addresses(&["[100::1]:9", "192.0.2.1:9"]);
        let started = std::time::Instant::now();
        let stream = happy_eyeballs(vec![silent[0], silent[1], open])
            .await
            .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), open);
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn every_address_refusing_is_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed = listener.local_addr().unwrap();
        drop(listener);
        assert!(happy_eyeballs(vec![closed]).await.is_err());
        assert!(happy_eyeballs(Vec::new()).await.is_err());
    }

    #[test]
    fn a_route_ela_cannot_take_is_refused_rather_than_skipped() {
        for key in ["proxy_jump", "proxy_command"] {
            let error = refuse_unsupported_route(&config(json!({ key: "gate" }))).unwrap_err();
            assert!(error.to_string().contains("did not connect directly"));
        }
    }

    #[test]
    fn no_route_or_an_empty_one_goes_direct() {
        assert!(refuse_unsupported_route(&config(json!({}))).is_ok());
        assert!(refuse_unsupported_route(&config(json!({ "proxy_jump": "  " }))).is_ok());
        assert!(refuse_unsupported_route(&config(json!({ "proxy_jump": "none" }))).is_ok());
    }

    #[test]
    fn jump_hosts_that_were_looked_up_are_taken() {
        let mut routed = config(json!({ "proxy_jump": "gate" }));
        routed.jump_hosts.push(JumpHost {
            name: "gate".into(),
            config: config(json!({})),
        });
        assert!(refuse_unsupported_route(&routed).is_ok());
        routed.options = json!({ "proxy_command": "nc %h %p" })
            .as_object()
            .unwrap()
            .clone();
        assert!(refuse_unsupported_route(&routed).is_err());
    }
}
