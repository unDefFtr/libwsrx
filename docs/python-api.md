# Python API 参考

[README](../README.md) · [使用指南](guide.md) · [配置参考](configuration.md) · [部署指南](deployment.md)

Python 扩展面向 Python 3.9+ 的 `asyncio` 应用。异步函数返回可 await 的对象，并在运行时使用 Tokio 执行网络 I/O；调用方仍按普通 asyncio task 管理取消与关闭。

模块导出 `Config`、`ClientEndpoint`、`WSRXError`、`run_client` 和 `run_server`。

## `Config`

```python
libwsrx.Config(
    *,
    tcp_read_buffer_size=65_536,
    max_websocket_message_size=67_108_864,
    max_websocket_frame_size=16_777_216,
    connect_timeout=10.0,
    handshake_timeout=10.0,
    max_concurrent_tunnels=1_024,
)
```

创建冻结配置对象。属性与关键字参数同名，可读取但不能修改。所有整数字段必须为正数并且可表示为 Rust `usize`；两项 WebSocket 大小字段也可设为 `None` 以取消对应限制。两个超时必须是有限且大于零的秒数。参数无效时构造函数抛出 `ValueError`。

完整字段说明、默认值和调优见[配置参考](configuration.md)。

## `ClientEndpoint`

### `await ClientEndpoint.bind(local_addr, websocket_url, *, config=None)`

绑定本地 TCP 监听器，并为每条源 TCP 连接建立到 `websocket_url` 的 WebSocket 隧道。`local_addr` 与 `websocket_url` 为字符串；传入 `127.0.0.1:0` 可请求随机本地端口。未提供 `config` 时使用默认 `Config`。

成功时返回端点对象，失败时抛出 `WSRXError`。URL 必须为有效 `ws://` 或 `wss://` URL。

### 属性

- `endpoint.local_addr`：实际绑定地址字符串，例如 `127.0.0.1:54321`。
- `endpoint.websocket_url`：创建端点时使用的 WebSocket URL 字符串。

### `await endpoint.shutdown()`

停止该端点的监听器并等待活动隧道结束。它是幂等的：调用多次均成功并返回 `None`。端点关闭不会影响同一进程中的其他端点。需要可观察的端口和受控关闭时，优先使用 `ClientEndpoint`，而不是 `run_client`。

```python
import asyncio

endpoint = await libwsrx.ClientEndpoint.bind(
    "127.0.0.1:0",
    "ws://127.0.0.1:9000",
)
try:
    print(endpoint.local_addr)
finally:
    await endpoint.shutdown()
```

## 长期运行函数

### `await run_client(local_addr, websocket_url, *, config=None)`

绑定本地源 TCP 监听器，并持续把每条连接转发至 `websocket_url`。该 awaitable 正常情况下持续运行；应用应保存其 task，并在停止时取消该 task。

### `await run_server(websocket_addr, target_addr, *, config=None)`

绑定 WebSocket 服务端监听器，并持续将每条已升级连接转发到固定的 `target_addr`。`target_addr` 不由客户端选择。该 API 接收原始 TCP，不提供 TLS 终止；公开 `wss://` 时在前方部署 TLS 终止层。

推荐将长期函数显式包装为 task，并在退出时取消和等待：

```python
import asyncio

server = asyncio.create_task(
    libwsrx.run_server("127.0.0.1:9000", "127.0.0.1:9100")
)
try:
    await serve_application()
finally:
    server.cancel()
    await asyncio.gather(server, return_exceptions=True)
```

取消这些 awaitable 时保持 Python 的 `asyncio.CancelledError`，而不是转换为 `WSRXError`。取消后，端点管理的活跃隧道会结束，连接另一端可观察到 EOF/关闭。

## 异常契约

| 异常 | 何时出现 | 调用方处理 |
| --- | --- | --- |
| `ValueError` | 创建 `Config` 时参数无效。 | 修正配置；不会开始网络操作。 |
| `WSRXError` | 绑定、连接、WebSocket handshake、目标 TCP 连接、relay 或受管理端点关闭发生运行时错误。 | 记录错误，并根据应用协议决定重试、告警或停止。 |
| `asyncio.CancelledError` | 调用方取消 `run_client`、`run_server`、`ClientEndpoint.bind` 或 `shutdown` 的 awaitable。 | 允许取消向上传播，或在应用关闭逻辑中显式处理。 |

单条隧道失败不会使长期 `run_client`/`run_server` 结束；库记录一条警告并继续处理其他连接。监听器自身的错误会使对应 awaitable 抛出 `WSRXError`。协议限制和关闭语义见[使用指南](guide.md)。
