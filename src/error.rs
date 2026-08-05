use std::time::Duration;

/// Errors produced while configuring, establishing, or relaying a tunnel.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// TCP, listener, or transport I/O failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// WebSocket URL handling, handshake, framing, or transport failed.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    /// A managed client endpoint task could not be joined.
    #[error("endpoint task failed: {0}")]
    EndpointTask(#[from] tokio::task::JoinError),

    /// A configuration field violated its documented constraint.
    #[error("invalid configuration for {field}: {reason}")]
    InvalidConfig {
        /// Name of the invalid configuration field.
        field: &'static str,
        /// Stable description of the violated constraint.
        reason: &'static str,
    },

    /// A text message was received where only binary messages are valid.
    #[error("WSRX only accepts binary WebSocket messages")]
    UnsupportedText,

    /// A raw frame unexpectedly escaped normal WebSocket processing.
    #[error("WSRX received an unexpected raw WebSocket frame")]
    UnexpectedRawFrame,

    /// Establishing the outbound WebSocket exceeded the configured duration.
    #[error("WebSocket connection timed out after {0:?}")]
    WebSocketConnectTimeout(Duration),

    /// Accepting the inbound WebSocket handshake exceeded the configured duration.
    #[error("WebSocket handshake timed out after {0:?}")]
    WebSocketHandshakeTimeout(Duration),

    /// Establishing the target TCP connection exceeded the configured duration.
    #[error("target TCP connection timed out after {0:?}")]
    TargetConnectTimeout(Duration),
}

/// Library result type using [`Error`] by default.
pub type Result<T, E = Error> = std::result::Result<T, E>;
