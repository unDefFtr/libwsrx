use futures_util::SinkExt;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    time::timeout,
};
use tokio_tungstenite::{WebSocketStream, accept_async_with_config};

use crate::{Config, Error, Result, endpoint::serve_connections, relay};

pub async fn accept<T>(websocket_transport: T, target_addr: &str, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    config.validate()?;

    let handshake = accept_async_with_config(websocket_transport, Some(config.websocket_config()));
    let mut websocket = timeout(config.handshake_timeout, handshake)
        .await
        .map_err(|_| Error::WebSocketHandshakeTimeout(config.handshake_timeout))??;

    let target = match timeout(config.connect_timeout, TcpStream::connect(target_addr)).await {
        Ok(Ok(target)) => target,
        Ok(Err(error)) => {
            close_best_effort(&mut websocket).await;
            return Err(error.into());
        }
        Err(_) => {
            close_best_effort(&mut websocket).await;
            return Err(Error::TargetConnectTimeout(config.connect_timeout));
        }
    };

    relay(target, websocket, config).await
}

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

pub async fn bind_and_serve<A>(listen_addr: A, target_addr: String, config: Config) -> Result<()>
where
    A: ToSocketAddrs,
{
    config.validate()?;
    let listener = TcpListener::bind(listen_addr).await?;
    serve(listener, target_addr, config).await
}

async fn close_best_effort<T>(websocket: &mut WebSocketStream<T>)
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let _ = websocket.close(None).await;
    let _ = websocket.flush().await;
}
