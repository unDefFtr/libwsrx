use std::time::Duration;

use futures_util::SinkExt;
use libwsrx::{Config, Error, client, server};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const BANNER: &[u8] = b"BANNER\n";

async fn start_echo_target(listener: TcpListener, banner: &'static [u8], tunnel_count: usize) {
    let mut tunnels = JoinSet::new();
    for _ in 0..tunnel_count {
        let (mut stream, _) = listener.accept().await.unwrap();
        tunnels.spawn(async move {
            stream.write_all(banner).await.unwrap();
            let (mut read, mut write) = stream.into_split();
            tokio::io::copy(&mut read, &mut write).await.unwrap();
        });
    }
    while tunnels.join_next().await.is_some() {}
}

async fn exercise_source(
    mut stream: TcpStream,
    expected_banner: &[u8],
    payload: Vec<u8>,
) -> TcpStream {
    let mut banner = vec![0; expected_banner.len()];
    stream.read_exact(&mut banner).await.unwrap();
    assert_eq!(banner, expected_banner);

    exchange_payload(&mut stream, &payload).await;
    stream
}

async fn exchange_payload(stream: &mut TcpStream, payload: &[u8]) {
    stream.write_all(payload).await.unwrap();
    let mut echoed = vec![0; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    assert_eq!(echoed, payload);
}

#[tokio::test]
async fn serves_concurrent_isolated_full_duplex_tunnels_and_cancels_them() {
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();
    let websocket_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let websocket_addr = websocket_listener.local_addr().unwrap();
    let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_listener.local_addr().unwrap();

    let target_task = tokio::spawn(start_echo_target(target_listener, BANNER, 2));
    let server_task = tokio::spawn(server::serve(
        websocket_listener,
        target_addr.to_string(),
        Config::default(),
    ));
    let client_task = tokio::spawn(client::serve(
        local_listener,
        format!("ws://{websocket_addr}"),
        Config::default(),
    ));

    let source_one = TcpStream::connect(local_addr).await.unwrap();
    let source_two = TcpStream::connect(local_addr).await.unwrap();
    let payload_one: Vec<u8> = (0..70_123).map(|index| (index % 251) as u8).collect();
    let payload_two: Vec<u8> = (0..68_765)
        .map(|index| 255_u8.wrapping_sub((index % 239) as u8))
        .collect();

    let (mut source_one, mut source_two) = tokio::time::timeout(TEST_TIMEOUT, async {
        tokio::join!(
            exercise_source(source_one, BANNER, payload_one),
            exercise_source(source_two, BANNER, payload_two),
        )
    })
    .await
    .expect("tunnels timed out");

    client_task.abort();
    server_task.abort();
    assert!(client_task.await.unwrap_err().is_cancelled());
    assert!(server_task.await.unwrap_err().is_cancelled());

    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), source_one.read(&mut byte))
            .await
            .expect("first source did not close")
            .unwrap(),
        0
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), source_two.read(&mut byte))
            .await
            .expect("second source did not close")
            .unwrap(),
        0
    );

    target_task.abort();
    let _ = target_task.await;
}

#[tokio::test]
async fn client_endpoints_shut_down_independently() {
    const FIRST_BANNER: &[u8] = b"FIRST\n";
    const SECOND_BANNER: &[u8] = b"SECOND\n";

    let first_target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_target_addr = first_target_listener.local_addr().unwrap();
    let second_target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_target_addr = second_target_listener.local_addr().unwrap();
    let first_websocket_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_websocket_addr = first_websocket_listener.local_addr().unwrap();
    let second_websocket_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_websocket_addr = second_websocket_listener.local_addr().unwrap();

    let first_target_task = tokio::spawn(start_echo_target(first_target_listener, FIRST_BANNER, 1));
    let second_target_task =
        tokio::spawn(start_echo_target(second_target_listener, SECOND_BANNER, 1));
    let first_server_task = tokio::spawn(server::serve(
        first_websocket_listener,
        first_target_addr.to_string(),
        Config::default(),
    ));
    let second_server_task = tokio::spawn(server::serve(
        second_websocket_listener,
        second_target_addr.to_string(),
        Config::default(),
    ));

    let first_url = format!("ws://{first_websocket_addr}");
    let second_url = format!("ws://{second_websocket_addr}");
    let first_endpoint =
        client::ClientEndpoint::bind("127.0.0.1:0", first_url.clone(), Config::default())
            .await
            .unwrap();
    let second_endpoint =
        client::ClientEndpoint::bind("127.0.0.1:0", second_url.clone(), Config::default())
            .await
            .unwrap();
    let first_local_addr = first_endpoint.local_addr();
    let second_local_addr = second_endpoint.local_addr();

    assert_ne!(first_local_addr.port(), 0);
    assert_ne!(second_local_addr.port(), 0);
    assert_ne!(first_local_addr, second_local_addr);
    assert_eq!(first_endpoint.websocket_url(), first_url);
    assert_eq!(second_endpoint.websocket_url(), second_url);

    let first_source = TcpStream::connect(first_local_addr).await.unwrap();
    let second_source = TcpStream::connect(second_local_addr).await.unwrap();
    let (mut first_source, mut second_source) = tokio::time::timeout(TEST_TIMEOUT, async {
        tokio::join!(
            exercise_source(first_source, FIRST_BANNER, b"first-payload".to_vec()),
            exercise_source(second_source, SECOND_BANNER, b"second-payload".to_vec(),),
        )
    })
    .await
    .expect("independent tunnels timed out");

    drop(first_endpoint);
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(TEST_TIMEOUT, first_source.read(&mut byte))
            .await
            .expect("first source did not close")
            .unwrap(),
        0
    );
    assert!(
        tokio::time::timeout(TEST_TIMEOUT, TcpStream::connect(first_local_addr))
            .await
            .expect("first endpoint reconnect timed out")
            .is_err()
    );

    exchange_payload(&mut second_source, b"second-still-live").await;
    second_endpoint.shutdown().await.unwrap();
    assert_eq!(
        tokio::time::timeout(TEST_TIMEOUT, second_source.read(&mut byte))
            .await
            .expect("second source did not close")
            .unwrap(),
        0
    );

    first_server_task.abort();
    second_server_task.abort();
    first_target_task.abort();
    second_target_task.abort();
    let _ = tokio::join!(
        first_server_task,
        second_server_task,
        first_target_task,
        second_target_task,
    );
}

#[tokio::test]
async fn limits_active_tunnels_before_connecting_another_target() {
    let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target_listener.local_addr().unwrap();
    let websocket_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let websocket_addr = websocket_listener.local_addr().unwrap();
    let config = Config {
        max_concurrent_tunnels: 1,
        ..Config::default()
    };
    let server_task = tokio::spawn(server::serve(
        websocket_listener,
        target_addr.to_string(),
        config,
    ));

    let (mut first_websocket, _) = tokio::time::timeout(
        TEST_TIMEOUT,
        connect_async(format!("ws://{websocket_addr}")),
    )
    .await
    .unwrap()
    .unwrap();
    let (first_target, _) = tokio::time::timeout(TEST_TIMEOUT, target_listener.accept())
        .await
        .unwrap()
        .unwrap();

    let second_connect = tokio::spawn(connect_async(format!("ws://{websocket_addr}")));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), target_listener.accept())
            .await
            .is_err()
    );

    first_websocket.close(None).await.unwrap();
    drop(first_target);

    let (mut second_websocket, _) = tokio::time::timeout(TEST_TIMEOUT, second_connect)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let (second_target, _) = tokio::time::timeout(TEST_TIMEOUT, target_listener.accept())
        .await
        .unwrap()
        .unwrap();

    second_websocket
        .send(Message::Binary(b"active".as_slice().into()))
        .await
        .unwrap();
    second_websocket.close(None).await.unwrap();
    drop(second_target);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test]
async fn times_out_an_idle_server_handshake() {
    let (server_transport, _idle_peer) = tokio::io::duplex(64);
    let timeout_duration = Duration::from_millis(25);
    let config = Config {
        handshake_timeout: timeout_duration,
        ..Config::default()
    };

    let result = server::accept(server_transport, "127.0.0.1:9", &config).await;
    assert!(matches!(
        result,
        Err(Error::WebSocketHandshakeTimeout(duration)) if duration == timeout_duration
    ));
}

#[tokio::test]
async fn wss_client_installs_a_provider_and_advertises_tls12() {
    let tls_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tls_addr = tls_listener.local_addr().unwrap();
    let peer_task = tokio::spawn(async move {
        let (mut stream, _) = tls_listener.accept().await.unwrap();
        let mut client_hello = [0; 4096];
        let bytes_read = stream.read(&mut client_hello).await.unwrap();
        client_hello[..bytes_read].to_vec()
    });
    let (relay_tcp, _source_tcp) = tokio::io::duplex(64);

    let result = client::connect(relay_tcp, &format!("wss://{tls_addr}"), &Config::default()).await;
    let client_hello = peer_task.await.unwrap();

    assert!(matches!(result, Err(Error::WebSocket(_))));
    assert!(
        client_hello
            .windows(9)
            .any(|window| { window == [0x00, 0x2b, 0x00, 0x05, 0x04, 0x03, 0x04, 0x03, 0x03] })
    );
}

#[test]
fn rejects_every_zero_configuration_value() {
    let cases: [(&str, Config); 6] = [
        (
            "tcp_read_buffer_size",
            Config {
                tcp_read_buffer_size: 0,
                ..Config::default()
            },
        ),
        (
            "max_websocket_message_size",
            Config {
                max_websocket_message_size: Some(0),
                ..Config::default()
            },
        ),
        (
            "max_websocket_frame_size",
            Config {
                max_websocket_frame_size: Some(0),
                ..Config::default()
            },
        ),
        (
            "connect_timeout",
            Config {
                connect_timeout: Duration::ZERO,
                ..Config::default()
            },
        ),
        (
            "handshake_timeout",
            Config {
                handshake_timeout: Duration::ZERO,
                ..Config::default()
            },
        ),
        (
            "max_concurrent_tunnels",
            Config {
                max_concurrent_tunnels: 0,
                ..Config::default()
            },
        ),
    ];

    for (expected_field, config) in cases {
        assert!(matches!(
            config.validate(),
            Err(Error::InvalidConfig { field, .. }) if field == expected_field
        ));
    }

    assert!(
        Config {
            max_websocket_message_size: None,
            max_websocket_frame_size: None,
            ..Config::default()
        }
        .validate()
        .is_ok()
    );
}
