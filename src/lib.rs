//! Embeddable TCP-over-WebSocket tunnels for Tokio applications.
//!
//! Each TCP connection maps to one WebSocket connection and one fixed target TCP
//! connection. Binary WebSocket messages carry raw TCP bytes; message boundaries
//! are not application framing.
#![deny(missing_docs)]

mod config;
mod endpoint;
mod error;
mod relay;

pub mod client;
pub mod server;

#[cfg(feature = "python")]
mod python;

pub use config::{Config, MAX_TCP_READ_BUFFER_SIZE};
pub use error::{Error, Result};
pub use relay::relay;
