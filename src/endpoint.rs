use std::future::Future;

use tokio::{
    net::{TcpListener, TcpStream},
    task::{JoinError, JoinSet},
};

use crate::Result;

pub(crate) async fn serve_connections<F, Fut>(
    listener: TcpListener,
    max_concurrent_tunnels: usize,
    handler: F,
) -> Result<()>
where
    F: Fn(TcpStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    serve_connections_until(
        listener,
        max_concurrent_tunnels,
        std::future::pending(),
        handler,
    )
    .await
}

pub(crate) async fn serve_connections_until<F, Fut, S>(
    listener: TcpListener,
    max_concurrent_tunnels: usize,
    shutdown: S,
    handler: F,
) -> Result<()>
where
    F: Fn(TcpStream) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
    S: Future<Output = ()>,
{
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        if tasks.len() >= max_concurrent_tunnels {
            tokio::select! {
                _ = &mut shutdown => break,
                completion = tasks.join_next() => log_completion(completion),
            }
            continue;
        }

        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let future = handler(stream);
                tasks.spawn(async move { (peer, future.await) });
            }
            completion = tasks.join_next(), if !tasks.is_empty() => {
                log_completion(completion);
            }
        }
    }

    tasks.shutdown().await;
    Ok(())
}

fn log_completion(
    completion: Option<std::result::Result<(std::net::SocketAddr, Result<()>), JoinError>>,
) {
    match completion {
        Some(Ok((_peer, Ok(())))) => {}
        Some(Ok((peer, Err(error)))) => {
            tracing::warn!(%peer, %error, "WSRX tunnel failed");
        }
        Some(Err(error)) => {
            tracing::warn!(%error, "WSRX tunnel task failed");
        }
        None => {}
    }
}
