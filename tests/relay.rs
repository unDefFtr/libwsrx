use std::{future::Future, time::Duration};

use futures_util::{SinkExt, StreamExt};
use libwsrx::{Config, Error, relay};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Error as WebSocketError, Message,
        protocol::{Role, WebSocketConfig, frame::coding::CloseCode},
    },
};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);

async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(TEST_TIMEOUT, future)
        .await
        .expect("operation timed out")
}

async fn raw_websocket_pair(
    config: Option<WebSocketConfig>,
) -> (WebSocketStream<DuplexStream>, WebSocketStream<DuplexStream>) {
    let (relay_io, peer_io) = tokio::io::duplex(1 << 20);
    tokio::join!(
        WebSocketStream::from_raw_socket(relay_io, Role::Server, config),
        WebSocketStream::from_raw_socket(peer_io, Role::Client, None),
    )
}

async fn relay_harness(
    config: Config,
) -> (
    DuplexStream,
    WebSocketStream<DuplexStream>,
    tokio::task::JoinHandle<libwsrx::Result<()>>,
) {
    let (relay_tcp, application_tcp) = tokio::io::duplex(1 << 20);
    let (relay_websocket, peer_websocket) =
        raw_websocket_pair(Some(config.websocket_config_for_test())).await;
    let relay_task = tokio::spawn(async move { relay(relay_tcp, relay_websocket, &config).await });
    (application_tcp, peer_websocket, relay_task)
}

trait TestWebSocketConfig {
    fn websocket_config_for_test(&self) -> WebSocketConfig;
}

impl TestWebSocketConfig for Config {
    fn websocket_config_for_test(&self) -> WebSocketConfig {
        WebSocketConfig::default()
            .max_message_size(self.max_websocket_message_size)
            .max_frame_size(self.max_websocket_frame_size)
    }
}

#[tokio::test]
async fn preserves_bytes_across_message_boundaries_in_both_directions() {
    let (mut tcp, mut websocket, relay_task) = relay_harness(Config::default()).await;

    websocket
        .send(Message::Binary(b"A".as_slice().into()))
        .await
        .unwrap();
    websocket
        .send(Message::Binary(b"BCDE".as_slice().into()))
        .await
        .unwrap();
    websocket
        .send(Message::Binary(b"F".as_slice().into()))
        .await
        .unwrap();

    let mut websocket_to_tcp = [0; 6];
    within(tcp.read_exact(&mut websocket_to_tcp)).await.unwrap();
    assert_eq!(&websocket_to_tcp, b"ABCDEF");

    let expected_tcp_to_websocket = [0x00, 0x41, 0xff, 0x42, 0x43, 0x44];
    tcp.write_all(&expected_tcp_to_websocket[..2])
        .await
        .unwrap();
    tcp.write_all(&expected_tcp_to_websocket[2..5])
        .await
        .unwrap();
    tcp.write_all(&expected_tcp_to_websocket[5..])
        .await
        .unwrap();

    let mut observed_tcp_to_websocket = Vec::new();
    while observed_tcp_to_websocket.len() < expected_tcp_to_websocket.len() {
        match within(websocket.next()).await.unwrap().unwrap() {
            Message::Binary(payload) => observed_tcp_to_websocket.extend_from_slice(&payload),
            message => panic!("unexpected WebSocket message: {message:?}"),
        }
    }
    assert_eq!(observed_tcp_to_websocket, expected_tcp_to_websocket);

    websocket.close(None).await.unwrap();
    assert!(within(relay_task).await.unwrap().is_ok());
}

#[tokio::test]
async fn tcp_to_websocket_respects_configured_read_buffer_size() {
    let config = Config {
        tcp_read_buffer_size: 3,
        ..Config::default()
    };
    let (mut tcp, mut websocket, relay_task) = relay_harness(config).await;
    let expected = b"more than three bytes";

    tcp.write_all(expected).await.unwrap();

    let mut observed = Vec::new();
    while observed.len() < expected.len() {
        match within(websocket.next()).await.unwrap().unwrap() {
            Message::Binary(payload) => {
                assert!(payload.len() <= 3);
                observed.extend_from_slice(&payload);
            }
            message => panic!("unexpected WebSocket message: {message:?}"),
        }
    }
    assert_eq!(observed, expected);

    websocket.close(None).await.unwrap();
    assert!(within(relay_task).await.unwrap().is_ok());
}

#[tokio::test]
async fn relays_both_directions_concurrently() {
    let (mut tcp, mut websocket, relay_task) = relay_harness(Config::default()).await;
    let tcp_payload = vec![0x5a; 32 * 1024];
    let websocket_payload = vec![0xa5; 24 * 1024];

    let send_tcp = tcp.write_all(&tcp_payload);
    let send_websocket = websocket.send(Message::Binary(websocket_payload.clone().into()));
    let (_, _) = within(async { tokio::join!(send_tcp, send_websocket) }).await;

    let receive_tcp = async {
        let mut received = vec![0; websocket_payload.len()];
        tcp.read_exact(&mut received).await.unwrap();
        received
    };
    let receive_websocket = async {
        let mut received = Vec::new();
        while received.len() < tcp_payload.len() {
            match websocket.next().await.unwrap().unwrap() {
                Message::Binary(payload) => received.extend_from_slice(&payload),
                message => panic!("unexpected WebSocket message: {message:?}"),
            }
        }
        received
    };
    let (received_tcp, received_websocket) =
        within(async { tokio::join!(receive_tcp, receive_websocket) }).await;

    assert_eq!(received_tcp, websocket_payload);
    assert_eq!(received_websocket, tcp_payload);

    websocket.close(None).await.unwrap();
    assert!(within(relay_task).await.unwrap().is_ok());
}

#[tokio::test]
async fn ignores_empty_binary_and_handles_ping_without_tcp_data() {
    let (mut tcp, mut websocket, relay_task) = relay_harness(Config::default()).await;

    websocket
        .send(Message::Binary(Vec::new().into()))
        .await
        .unwrap();
    let ping_payload = b"still-alive".as_slice();
    websocket
        .send(Message::Ping(ping_payload.into()))
        .await
        .unwrap();

    let pong = within(websocket.next()).await.unwrap().unwrap();
    assert_eq!(pong, Message::Pong(ping_payload.into()));

    let mut byte = [0];
    assert!(
        tokio::time::timeout(Duration::from_millis(50), tcp.read(&mut byte))
            .await
            .is_err()
    );

    websocket
        .send(Message::Binary(b"X".as_slice().into()))
        .await
        .unwrap();
    within(tcp.read_exact(&mut byte)).await.unwrap();
    assert_eq!(&byte, b"X");

    websocket.close(None).await.unwrap();
    assert!(within(relay_task).await.unwrap().is_ok());
}

#[tokio::test]
async fn rejects_text_with_unsupported_close() {
    let (_tcp, mut websocket, relay_task) = relay_harness(Config::default()).await;

    websocket
        .send(Message::Text("not binary".into()))
        .await
        .unwrap();
    let close = within(websocket.next()).await.unwrap().unwrap();
    let Message::Close(Some(frame)) = close else {
        panic!("expected a close frame, got {close:?}");
    };
    assert_eq!(frame.code, CloseCode::Unsupported);
    assert_eq!(frame.reason, "WSRX only accepts binary WebSocket messages");

    assert!(matches!(
        within(relay_task).await.unwrap(),
        Err(Error::UnsupportedText)
    ));
}

#[tokio::test]
async fn enforces_custom_websocket_message_limit() {
    let config = Config {
        max_websocket_message_size: Some(4),
        max_websocket_frame_size: Some(64),
        ..Config::default()
    };
    let (_tcp, mut websocket, relay_task) = relay_harness(config).await;

    websocket
        .send(Message::Binary(b"12345".as_slice().into()))
        .await
        .unwrap();
    let close = within(websocket.next()).await.unwrap().unwrap();
    let Message::Close(Some(frame)) = close else {
        panic!("expected a close frame, got {close:?}");
    };
    assert_eq!(frame.code, CloseCode::Size);

    assert!(matches!(
        within(relay_task).await.unwrap(),
        Err(Error::WebSocket(WebSocketError::Capacity(_)))
    ));
}
