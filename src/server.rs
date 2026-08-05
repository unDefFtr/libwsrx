//! Server-side WebSocket acceptance and fixed-target TCP forwarding APIs.

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    time::timeout,
};
use tokio_tungstenite::accept_async_with_config;

use crate::{Config, Error, Result, endpoint::serve_connections, relay};

/// Accepts one WebSocket transport, connects the fixed TCP target, and relays bytes.
///
/// # Errors
///
/// Returns an error for invalid configuration, handshake or target connection
/// timeout, WebSocket failure, target TCP I/O failure, or relay protocol violation.
pub async fn accept<T>(websocket_transport: T, target_addr: &str, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    config.validate()?;

    let handshake = accept_async_with_config(websocket_transport, Some(config.websocket_config()));
    let websocket = timeout(config.handshake_timeout, handshake)
        .await
        .map_err(|_| Error::WebSocketHandshakeTimeout(config.handshake_timeout))??;

    let target = match timeout(config.connect_timeout, TcpStream::connect(target_addr)).await {
        Ok(Ok(target)) => target,
        Ok(Err(error)) => return Err(error.into()),
        Err(_) => return Err(Error::TargetConnectTimeout(config.connect_timeout)),
    };

    relay(target, websocket, config).await
}

/// Serves a pre-bound WebSocket listener until failure or cancellation.
///
/// Individual tunnel failures are logged and do not stop the listener.
///
/// # Errors
///
/// Returns an error for invalid configuration or a listener accept failure.
pub async fn serve(listener: TcpListener, target_addr: String, config: Config) -> Result<()> {
    config.validate()?;
    let max_concurrent_tunnels = config.max_concurrent_tunnels;

    serve_connections(
        listener,
        max_concurrent_tunnels,
        move |websocket_transport| {
            let target_addr = target_addr.clone();
            let config = config.clone();
            async move { accept(websocket_transport, &target_addr, &config).await }
        },
    )
    .await
}

/// Binds and serves a server-side WebSocket listener for a fixed TCP target.
///
/// # Errors
///
/// Returns an error for invalid configuration, listener binding, or a subsequent
/// listener accept failure.
pub async fn bind_and_serve<A>(listen_addr: A, target_addr: String, config: Config) -> Result<()>
where
    A: ToSocketAddrs,
{
    config.validate()?;
    let listener = TcpListener::bind(listen_addr).await?;
    serve(listener, target_addr, config).await
}
