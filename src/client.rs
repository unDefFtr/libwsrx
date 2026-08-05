//! Client-side listener and outbound WebSocket connection APIs.

use std::net::SocketAddr;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, ToSocketAddrs},
    sync::oneshot,
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Error as WebSocketError,
        client::{IntoClientRequest, uri_mode},
        error::UrlError,
    },
};

use crate::{
    Config, Error, Result,
    endpoint::{serve_connections, serve_connections_until},
    relay,
};

fn validate_websocket_url(websocket_url: &str) -> Result<()> {
    let request = websocket_url.into_client_request();
    if websocket_url
        .strip_prefix("ws://")
        .or_else(|| websocket_url.strip_prefix("wss://"))
        .is_some_and(|remainder| remainder.starts_with('/'))
    {
        return Err(WebSocketError::Url(UrlError::NoHostName).into());
    }
    let request = request?;
    uri_mode(request.uri())?;
    Ok(())
}

/// Managed client listener with explicit shutdown and observable bound address.
///
/// Calling [`ClientEndpoint::shutdown`] cancels active tunnels and waits for the
/// management task. Dropping the value only signals shutdown and aborts that
/// task; it does not wait for cleanup or report task errors.
pub struct ClientEndpoint {
    local_addr: SocketAddr,
    websocket_url: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<()>>>,
}

impl ClientEndpoint {
    /// Binds a TCP listener and starts forwarding connections to a WebSocket URL.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration or URL, listener binding or
    /// address inspection failures, or endpoint startup failure.
    pub async fn bind<A>(listen_addr: A, websocket_url: String, config: Config) -> Result<Self>
    where
        A: ToSocketAddrs,
    {
        config.validate()?;
        validate_websocket_url(&websocket_url)?;
        let listener = TcpListener::bind(listen_addr).await?;
        Self::start(listener, websocket_url, config)
    }

    /// Starts a managed endpoint from a pre-bound TCP listener.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration or URL, or if the listener's
    /// local address cannot be read.
    pub fn start(listener: TcpListener, websocket_url: String, config: Config) -> Result<Self> {
        config.validate()?;
        validate_websocket_url(&websocket_url)?;
        let local_addr = listener.local_addr()?;
        let max_concurrent_tunnels = config.max_concurrent_tunnels;
        let endpoint_url = websocket_url.clone();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(serve_connections_until(
            listener,
            max_concurrent_tunnels,
            async move {
                let _ = shutdown_rx.await;
            },
            move |tcp| {
                let websocket_url = websocket_url.clone();
                let config = config.clone();
                async move { connect(tcp, &websocket_url, &config).await }
            },
        ));

        Ok(Self {
            local_addr,
            websocket_url: endpoint_url,
            shutdown_tx: Some(shutdown_tx),
            task: Some(task),
        })
    }

    /// Returns the TCP address actually bound by this endpoint.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the validated outbound WebSocket URL.
    pub fn websocket_url(&self) -> &str {
        &self.websocket_url
    }

    /// Stops accepting connections, cancels active tunnels, and waits for the management task.
    ///
    /// # Errors
    ///
    /// Returns an endpoint task join error or an error returned by the managed
    /// listener task.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        let Some(task) = self.task.as_mut() else {
            return Ok(());
        };
        let result = task.await?;
        self.task.take();
        result
    }
}

impl Drop for ClientEndpoint {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Connects one TCP transport to an outbound WebSocket and relays bytes.
///
/// # Errors
///
/// Returns an error for invalid configuration or URL, connection timeout,
/// WebSocket failure, TCP I/O failure, or relay protocol violation.
pub async fn connect<T>(tcp: T, websocket_url: &str, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    config.validate()?;
    validate_websocket_url(websocket_url)?;

    let connect = connect_async_with_config(websocket_url, Some(config.websocket_config()), true);
    let (websocket, _) = timeout(config.connect_timeout, connect)
        .await
        .map_err(|_| Error::WebSocketConnectTimeout(config.connect_timeout))??;

    relay(tcp, websocket, config).await
}

/// Serves a pre-bound TCP listener until failure or cancellation.
///
/// Individual tunnel failures are logged and do not stop the listener.
///
/// # Errors
///
/// Returns an error for invalid configuration or URL, or a listener accept failure.
pub async fn serve(listener: TcpListener, websocket_url: String, config: Config) -> Result<()> {
    config.validate()?;
    validate_websocket_url(&websocket_url)?;
    let max_concurrent_tunnels = config.max_concurrent_tunnels;

    serve_connections(listener, max_concurrent_tunnels, move |tcp| {
        let websocket_url = websocket_url.clone();
        let config = config.clone();
        async move { connect(tcp, &websocket_url, &config).await }
    })
    .await
}

/// Binds and serves a client-side TCP listener.
///
/// # Errors
///
/// Returns an error for invalid configuration or URL, listener binding, or a
/// subsequent listener accept failure.
pub async fn bind_and_serve<A>(listen_addr: A, websocket_url: String, config: Config) -> Result<()>
where
    A: ToSocketAddrs,
{
    config.validate()?;
    validate_websocket_url(&websocket_url)?;
    let listener = TcpListener::bind(listen_addr).await?;
    serve(listener, websocket_url, config).await
}
