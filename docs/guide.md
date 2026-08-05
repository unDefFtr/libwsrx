# 使用指南

[README](../README.md) · [部署指南](deployment.md) · [配置参考](configuration.md) · [Rust API](rust-api.md) · [Python API](python-api.md) · [核心连接协议](protocol.md)

本文面向把 `libwsrx` 嵌入应用或网关的使用者。它解释连接如何流动、哪些资源由调用方管理，以及隧道何时结束。数据编码的互操作性细节见[核心连接协议](protocol.md)。

## 心智模型

`libwsrx` 将单条源 TCP 连接映射为单条 WebSocket 连接；服务端再将该 WebSocket 映射为单条固定目标 TCP 连接。

```mermaid
flowchart LR
    S["源 TCP 应用"] -->|"1 条 TCP"| C["客户端端点"]
    C -->|"1 条 WebSocket"| W["WebSocket 服务端"]
    W -->|"1 条 TCP"| T["固定目标 TCP 服务"]
```

这三个连接共同构成一条隧道。一个监听端点可并发处理多条隧道，但它们互不共享 WebSocket、状态或字节流。`max_concurrent_tunnels` 限制的是每个端点正在处理的隧道数，而不是整个进程的全局上限。

客户端端点收到源 TCP 连接后才会拨出 WebSocket。服务端完成 WebSocket Upgrade 后才会连接目标 TCP 地址。因此，目标地址始终由服务端启动参数或调用方控制，不来自 WebSocket payload。

## 字节与消息

隧道在两个方向同时转发数据：

```text
源 TCP bytes -> WebSocket Binary Message -> 目标 TCP bytes
源 TCP bytes <- WebSocket Binary Message <- 目标 TCP bytes
```

每个方向的字节顺序会保留，但 TCP 的一次 `write` 或 `read` 不对应固定数量的 WebSocket Message，也不对应接收端的一次 `read`。上层 TCP 协议必须继续使用自己的长度字段、分隔符或其他 framing。

只允许 Binary Message 搬运 TCP 数据。Ping 和 Pong 是控制消息，不会交给 TCP 应用；Text Message、raw WebSocket frame 和超出大小限制的消息会终止当前隧道。具体关闭码与互操作规则见[核心连接协议](protocol.md)。

## 选择嵌入方式

高层 API 适合应用只需要提供一个长期运行的监听端点：

- Rust 使用 `client::bind_and_serve`、`server::bind_and_serve`，或需要显式关闭客户端时使用 `ClientEndpoint`。
- Python 使用 `run_client`、`run_server`，或需要实际随机端口和显式关闭时使用 `ClientEndpoint.bind`。

低层 API 适合调用方已有监听器、TCP 流或自己的连接生命周期：

- `client::serve` 接收预先绑定的本地 `TcpListener`；`client::connect` 接收一条源 TCP 流。
- `server::serve` 接收预先绑定的 WebSocket `TcpListener`；`server::accept` 接收一条已建立的 WebSocket 底层传输。
- `relay` 接收一条 TCP 流和已完成握手的 `WebSocketStream`。

完整签名、错误和示例见 [Rust API 参考](rust-api.md) 与 [Python API 参考](python-api.md)。

## 生命周期与资源所有权

高层服务函数会持续接受连接，直到其 task 被取消、发生监听器错误，或 Rust `ClientEndpoint` 收到 `shutdown`。每条隧道的任何一端结束、发生 I/O 错误、握手/连接超时或协议错误，都会结束该隧道并释放另一侧连接。TCP EOF 不映射为 half-close；它会终止整条隧道。

调用方应按所选择的 API 管理资源：

| 场景 | 调用方负责 | 库负责 |
| --- | --- | --- |
| `bind_and_serve` / `run_client` / `run_server` | 持有并取消长期运行的 future/task | 绑定监听器、接受连接和管理隧道 |
| Rust `ClientEndpoint::bind` | 在不再使用时调用 `shutdown(self).await`，或让对象 drop | 监听器与活跃隧道，直到 shutdown 或 drop |
| `serve` | 绑定并交出 `TcpListener` | 接受连接和管理隧道 |
| `connect` / `accept` / `relay` | 创建并交出单条传输 | 该次调用期间的双向 relay 和 WebSocket 关闭 |

Rust `ClientEndpoint` 被 drop 时会停止监听并中止其管理 task；需要等待清理完成、接收可能的端点错误时，应优先调用 `shutdown().await`。Python `ClientEndpoint.shutdown()` 可安全重复调用。长期 Python task 的推荐停止方式是取消 task 并 `await` 它，使 `asyncio.CancelledError` 由调用方处理。

## 运行边界

库不实现认证、授权、目标发现、负载均衡、自动重连、可靠重传、会话恢复、空闲超时或多路复用。每条隧道完成发送不代表目标应用已处理数据；上层协议必须自行定义确认语义。

客户端支持 `ws://` 和 `wss://`。高层服务端接收原始 TCP，不在库中终止 TLS；公开 `wss://` 入口时，应将 TLS 终止和 Upgrade 转发放在服务端前方。部署职责见[部署指南](deployment.md)。
