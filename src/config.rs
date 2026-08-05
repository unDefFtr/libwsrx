use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{Error, Result};
/// Maximum supported [`Config::tcp_read_buffer_size`] in bytes (16 MiB).
pub const MAX_TCP_READ_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Endpoint-local tunnel configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Maximum bytes read from TCP into one outbound binary message. Defaults to 65,536.
    pub tcp_read_buffer_size: usize,
    /// Maximum accepted WebSocket message size, or `None` for unlimited. Defaults to 67,108,864.
    pub max_websocket_message_size: Option<usize>,
    /// Maximum accepted WebSocket frame size, or `None` for unlimited. Defaults to 16,777,216.
    pub max_websocket_frame_size: Option<usize>,
    /// Timeout for outbound WebSocket and target TCP connections. Defaults to 10 seconds.
    pub connect_timeout: Duration,
    /// Timeout for an inbound WebSocket handshake. Defaults to 10 seconds.
    pub handshake_timeout: Duration,
    /// Maximum tunnels managed concurrently by one endpoint. Defaults to 1,024.
    pub max_concurrent_tunnels: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tcp_read_buffer_size: 65_536,
            max_websocket_message_size: Some(67_108_864),
            max_websocket_frame_size: Some(16_777_216),
            connect_timeout: Duration::from_secs(10),
            handshake_timeout: Duration::from_secs(10),
            max_concurrent_tunnels: 1_024,
        }
    }
}

impl Config {
    /// Validates every configuration constraint.
    ///
    /// `tcp_read_buffer_size` must be between 1 and
    /// [`MAX_TCP_READ_BUFFER_SIZE`] inclusive. Optional WebSocket limits may be
    /// `None`; all configured sizes, durations, and concurrency limits must be
    /// nonzero.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] naming the first invalid field.
    pub fn validate(&self) -> Result<()> {
        validate_nonzero("tcp_read_buffer_size", self.tcp_read_buffer_size)?;
        if self.tcp_read_buffer_size > MAX_TCP_READ_BUFFER_SIZE {
            return Err(Error::InvalidConfig {
                field: "tcp_read_buffer_size",
                reason: "must not exceed 16777216 bytes",
            });
        }
        validate_optional_nonzero(
            "max_websocket_message_size",
            self.max_websocket_message_size,
        )?;
        validate_optional_nonzero("max_websocket_frame_size", self.max_websocket_frame_size)?;
        validate_duration("connect_timeout", self.connect_timeout)?;
        validate_duration("handshake_timeout", self.handshake_timeout)?;
        validate_nonzero("max_concurrent_tunnels", self.max_concurrent_tunnels)?;
        Ok(())
    }

    pub(crate) fn websocket_config(&self) -> WebSocketConfig {
        WebSocketConfig::default()
            .max_message_size(self.max_websocket_message_size)
            .max_frame_size(self.max_websocket_frame_size)
    }
}

fn validate_nonzero(field: &'static str, value: usize) -> Result<()> {
    if value == 0 {
        return Err(Error::InvalidConfig {
            field,
            reason: "must be greater than zero",
        });
    }
    Ok(())
}

fn validate_optional_nonzero(field: &'static str, value: Option<usize>) -> Result<()> {
    if value == Some(0) {
        return Err(Error::InvalidConfig {
            field,
            reason: "must be greater than zero when set",
        });
    }
    Ok(())
}

fn validate_duration(field: &'static str, value: Duration) -> Result<()> {
    if value.is_zero() {
        return Err(Error::InvalidConfig {
            field,
            reason: "must be greater than zero",
        });
    }
    Ok(())
}
