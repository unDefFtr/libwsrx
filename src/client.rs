use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, ToSocketAddrs},
    time::timeout,
};
use tokio_tungstenite::connect_async_with_config;

use crate::{Config, Error, Result, endpoint::serve_connections, relay};

pub async fn connect<T>(tcp: T, websocket_url: &str, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    config.validate()?;

    let connect = connect_async_with_config(websocket_url, Some(config.websocket_config()), true);
    let (websocket, _) = timeout(config.connect_timeout, connect)
        .await
        .map_err(|_| Error::WebSocketConnectTimeout(config.connect_timeout))??;

    relay(tcp, websocket, config).await
}

pub async fn serve(listener: TcpListener, websocket_url: String, config: Config) -> Result<()> {
    config.validate()?;
    let max_concurrent_tunnels = config.max_concurrent_tunnels;

    serve_connections(listener, max_concurrent_tunnels, move |tcp| {
        let websocket_url = websocket_url.clone();
        let config = config.clone();
        async move { connect(tcp, &websocket_url, &config).await }
    })
    .await
}

pub async fn bind_and_serve<A>(listen_addr: A, websocket_url: String, config: Config) -> Result<()>
where
    A: ToSocketAddrs,
{
    config.validate()?;
    let listener = TcpListener::bind(listen_addr).await?;
    serve(listener, websocket_url, config).await
}
