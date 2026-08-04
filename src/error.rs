use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("invalid configuration for {field}: {reason}")]
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },

    #[error("WSRX only accepts binary WebSocket messages")]
    UnsupportedText,

    #[error("WSRX received an unexpected raw WebSocket frame")]
    UnexpectedRawFrame,

    #[error("WebSocket connection timed out after {0:?}")]
    WebSocketConnectTimeout(Duration),

    #[error("WebSocket handshake timed out after {0:?}")]
    WebSocketHandshakeTimeout(Duration),

    #[error("target TCP connection timed out after {0:?}")]
    TargetConnectTimeout(Duration),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
