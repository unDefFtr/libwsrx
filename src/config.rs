use std::time::Duration;

use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub tcp_read_buffer_size: usize,
    pub max_websocket_message_size: Option<usize>,
    pub max_websocket_frame_size: Option<usize>,
    pub connect_timeout: Duration,
    pub handshake_timeout: Duration,
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
    pub fn validate(&self) -> Result<()> {
        validate_nonzero("tcp_read_buffer_size", self.tcp_read_buffer_size)?;
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
