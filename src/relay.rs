use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        error::CapacityError,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};

use crate::{Config, Error, Result};

const UNSUPPORTED_TEXT_REASON: &str = "WSRX only accepts binary WebSocket messages";
const UNEXPECTED_FRAME_REASON: &str = "WSRX received an unexpected raw WebSocket frame";

pub async fn relay<T, W>(tcp: T, websocket: WebSocketStream<W>, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
    W: AsyncRead + AsyncWrite + Unpin,
{
    config.validate()?;

    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp);
    let (mut websocket_write, mut websocket_read) = websocket.split();

    let termination = {
        let tcp_to_websocket = tcp_to_websocket(
            &mut tcp_read,
            &mut websocket_write,
            config.tcp_read_buffer_size,
        );
        let websocket_to_tcp = websocket_to_tcp(&mut websocket_read, &mut tcp_write);
        tokio::pin!(tcp_to_websocket, websocket_to_tcp);

        tokio::select! {
            result = &mut tcp_to_websocket => result,
            result = &mut websocket_to_tcp => result,
        }
    };

    let mut websocket = websocket_write
        .reunite(websocket_read)
        .expect("WebSocket halves from the same split must reunite");
    let _ = websocket.close(termination.close_frame).await;
    let _ = websocket.flush().await;

    termination.result
}

async fn tcp_to_websocket<T, W>(
    tcp_read: &mut tokio::io::ReadHalf<T>,
    websocket_write: &mut SplitSink<WebSocketStream<W>, Message>,
    buffer_size: usize,
) -> Termination
where
    T: AsyncRead + AsyncWrite + Unpin,
    W: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let mut buffer = vec![0; buffer_size];
        let bytes_read = match tcp_read.read(&mut buffer).await {
            Ok(0) => return Termination::normal(),
            Ok(bytes_read) => bytes_read,
            Err(error) => return Termination::error(error.into()),
        };
        buffer.truncate(bytes_read);

        if let Err(error) = websocket_write.send(Message::Binary(buffer.into())).await {
            return Termination::error(error.into());
        }
    }
}

async fn websocket_to_tcp<T, W>(
    websocket_read: &mut SplitStream<WebSocketStream<W>>,
    tcp_write: &mut tokio::io::WriteHalf<T>,
) -> Termination
where
    T: AsyncRead + AsyncWrite + Unpin,
    W: AsyncRead + AsyncWrite + Unpin,
{
    while let Some(message) = websocket_read.next().await {
        match message {
            Ok(Message::Binary(payload)) => {
                if let Err(error) = tcp_write.write_all(&payload).await {
                    return Termination::error(error.into());
                }
            }
            Ok(Message::Text(_)) => {
                return Termination::with_close(
                    Error::UnsupportedText,
                    CloseCode::Unsupported,
                    UNSUPPORTED_TEXT_REASON,
                );
            }
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(Message::Close(_)) => return Termination::normal(),
            Ok(Message::Frame(_)) => {
                return Termination::with_close(
                    Error::UnexpectedRawFrame,
                    CloseCode::Protocol,
                    UNEXPECTED_FRAME_REASON,
                );
            }
            Err(
                error @ tokio_tungstenite::tungstenite::Error::Capacity(
                    CapacityError::MessageTooLong { .. },
                ),
            ) => {
                return Termination::with_close(error.into(), CloseCode::Size, "");
            }
            Err(error) => return Termination::error(error.into()),
        }
    }

    Termination::normal()
}

struct Termination {
    result: Result<()>,
    close_frame: Option<CloseFrame>,
}

impl Termination {
    fn normal() -> Self {
        Self {
            result: Ok(()),
            close_frame: None,
        }
    }

    fn error(error: Error) -> Self {
        Self {
            result: Err(error),
            close_frame: None,
        }
    }

    fn with_close(error: Error, code: CloseCode, reason: &'static str) -> Self {
        Self {
            result: Err(error),
            close_frame: Some(CloseFrame {
                code,
                reason: reason.into(),
            }),
        }
    }
}
