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
    let mut tasks = JoinSet::new();

    loop {
        if tasks.len() >= max_concurrent_tunnels {
            log_completion(tasks.join_next().await);
            continue;
        }

        tokio::select! {
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
