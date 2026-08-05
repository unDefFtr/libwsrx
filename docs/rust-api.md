# Rust API 参考

[README](../README.md) · [使用指南](guide.md) · [配置参考](configuration.md) · [核心连接协议](protocol.md)

所有 API 都运行在 Tokio 异步上下文中。公开入口会校验 `Config` 并返回 `libwsrx::Result<T>`。传输参数保持泛型：单条流只需实现 `AsyncRead + AsyncWrite + Unpin`，因此可以传入 Tokio TCP 流、TLS 包装流或测试传输。

## 根模块

### `Config`

`libwsrx::Config` 保存读取块、WebSocket 限制、连接/握手超时与并发上限。使用 `Config::default()` 获取默认值，使用 `config.validate()?` 在启动前验证。字段、默认值与调优见[配置参考](configuration.md)。

### `Result<T>` 与 `Error`

`Result<T>` 是 `std::result::Result<T, Error>` 的别名。`Error` 的变体如下：

| 变体 | 表示 |
| --- | --- |
| `Io` | 绑定、接受、目标连接或 relay 中发生的 I/O 错误。 |
| `WebSocket` | URL 解析、客户端连接、握手、帧或其他 tungstenite 错误。 |
| `EndpointTask` | 受管理的 `ClientEndpoint` task 无法正常 join。 |
| `InvalidConfig` | 配置为零或其他无效值。 |
| `UnsupportedText` | 对端发送 Text Message；本端发送 Unsupported close。 |
| `UnexpectedRawFrame` | 收到不应由正常 WebSocket API 暴露的 raw frame；本端发送 Protocol close。 |
| `WebSocketConnectTimeout` | 客户端在 `connect_timeout` 内未建立 WebSocket。 |
| `WebSocketHandshakeTimeout` | 服务端在 `handshake_timeout` 内未完成 Upgrade。 |
| `TargetConnectTimeout` | 服务端在 `connect_timeout` 内未连接固定目标 TCP。 |

端点服务循环将单条隧道失败通过 `tracing::warn!` 记录后继续服务其他连接；监听器的接受错误仍会从 `serve` 返回。

### `relay(tcp, websocket, config)`

```rust
pub async fn relay<T, W>(
    tcp: T,
    websocket: WebSocketStream<W>,
    config: &Config,
) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
    W: AsyncRead + AsyncWrite + Unpin;
```

在一条 TCP 流和一条已完成 WebSocket 握手的流之间双向 relay。调用者必须先完成客户端连接或服务端 Upgrade，并把两条传输的所有权交给函数。任一方向结束后函数关闭 WebSocket 并返回；它不提供 TCP half-close 映射。仅在需要自行控制握手和底层传输时直接使用，通常选择 `client` 或 `server` 模块。

## `client` 模块

客户端把源 TCP 连接发送到一个 `ws://` 或 `wss://` URL。URL 在开始监听前校验，缺少主机或使用非 WebSocket scheme 会返回 `Error::WebSocket`。

### `connect(tcp, websocket_url, config)`

```rust
pub async fn connect<T>(tcp: T, websocket_url: &str, config: &Config) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin;
```

使用调用方提供的一条源 TCP 流建立出站 WebSocket，并 relay 至该 WebSocket。函数拥有 `tcp` 直到隧道结束；WebSocket 建立受 `connect_timeout` 限制。用于调用方已接受或创建源流、但不想让库管理监听器的场景。

### `serve(listener, websocket_url, config)`

```rust
pub async fn serve(
    listener: tokio::net::TcpListener,
    websocket_url: String,
    config: Config,
) -> Result<()>;
```

接管预先绑定的源 TCP 监听器。每条入站连接调用 `connect`，并发数由 `max_concurrent_tunnels` 限制。函数通常持续运行，直到 task 被取消或监听器接受失败；调用方不再持有 listener。

### `bind_and_serve(listen_addr, websocket_url, config)`

```rust
pub async fn bind_and_serve<A>(
    listen_addr: A,
    websocket_url: String,
    config: Config,
) -> Result<()>
where
    A: tokio::net::ToSocketAddrs;
```

绑定源 TCP 地址后进入 `serve`。适合应用不需要实际分配端口或显式端点控制的长期 client task。`listen_addr`、URL 和配置会在服务循环前验证。

### `ClientEndpoint`

`ClientEndpoint` 适合需要随机端口、观察实际监听地址或受控停止的客户端监听器。

```rust
use libwsrx::{Config, client::ClientEndpoint};

let endpoint = ClientEndpoint::bind(
    "127.0.0.1:0",
    "ws://127.0.0.1:9000".to_owned(),
    Config::default(),
)
.await?;

println!("{}", endpoint.local_addr());
endpoint.shutdown().await?;
```

- `ClientEndpoint::bind(listen_addr, websocket_url, config).await` 绑定监听器并启动管理 task；可传入 `127.0.0.1:0` 获取随机端口。
- `ClientEndpoint::start(listener, websocket_url, config)` 接管调用方已经绑定的 `TcpListener`，但不再由调用方持有它。
- `local_addr()` 返回实际的 `SocketAddr`；`websocket_url()` 返回已验证并保存的 URL。
- `shutdown(self).await` 消耗 endpoint，停止接受新连接、取消活跃隧道并等待管理 task；无剩余 task 时返回成功。
- Drop 会发送停止信号并 abort 管理 task。Drop 不等待清理或返回任务错误，因此正常关停优先使用 `shutdown`。

## `server` 模块

服务端接受 WebSocket 底层传输，完成 Upgrade，然后连接固定目标 TCP。目标地址不来自客户端 payload。

### `accept(websocket_transport, target_addr, config)`

```rust
pub async fn accept<T>(
    websocket_transport: T,
    target_addr: &str,
    config: &Config,
) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin;
```

对调用方提供的底层传输执行服务端 WebSocket handshake，然后连接 `target_addr` 并 relay。handshake 受 `handshake_timeout` 限制，目标 TCP 连接受 `connect_timeout` 限制。适合调用方已在传输外层处理 TLS、PROXY protocol 或认证的场景。

### `serve(listener, target_addr, config)`

```rust
pub async fn serve(
    listener: tokio::net::TcpListener,
    target_addr: String,
    config: Config,
) -> Result<()>;
```

接管预先绑定的 WebSocket 服务端 TCP 监听器。每条连接调用 `accept`，并发数由 `max_concurrent_tunnels` 限制；函数持续到取消或监听器错误。

### `bind_and_serve(listen_addr, target_addr, config)`

```rust
pub async fn bind_and_serve<A>(
    listen_addr: A,
    target_addr: String,
    config: Config,
) -> Result<()>
where
    A: tokio::net::ToSocketAddrs;
```

绑定原始 TCP 监听地址后进入 `serve`。该 API 本身不终止 TLS；公开 `wss://` 时，使用外围 TLS 终止层，或调用 `accept` 并传入调用方处理后的传输。部署边界见[部署指南](deployment.md)。
