# 配置参考

[README](../README.md) · [使用指南](guide.md) · [部署指南](deployment.md) · [Rust API](rust-api.md) · [Python API](python-api.md)

`Config` 是端点本地、不可变地传入每个 Rust/Python 入口的配置。Rust 使用公共字段和 `std::time::Duration`；Python 使用同名关键字参数和浮点秒数。每个公共入口都会调用配置校验，因此无效配置不会进入接受或连接循环。

## 默认值与限制

| 字段 | 默认值 | Rust 类型 / Python 类型 | 约束与作用 |
| --- | ---: | --- | --- |
| `tcp_read_buffer_size` | 65,536 bytes | `usize` / `int` | 必须大于零。每次从 TCP 读取的最大字节数，并作为一条出站 Binary Message 的最大读取块。 |
| `max_websocket_message_size` | 67,108,864 bytes | `Option<usize>` / `int | None` | 设置时必须大于零。收到的单条 WebSocket Message 上限；`None` 表示不限制。 |
| `max_websocket_frame_size` | 16,777,216 bytes | `Option<usize>` / `int | None` | 设置时必须大于零。收到的单个 WebSocket frame 上限；`None` 表示不限制。 |
| `connect_timeout` | 10 seconds | `Duration` / `float` | 必须大于零。客户端建立 WebSocket、服务端建立目标 TCP 的最长时间。 |
| `handshake_timeout` | 10 seconds | `Duration` / `float` | 必须大于零。服务端完成 WebSocket Upgrade 的最长时间。 |
| `max_concurrent_tunnels` | 1,024 | `usize` / `int` | 必须大于零。单个端点可同时处理的隧道数。到达上限时暂停接受新连接，直到已有隧道结束。 |

Python 的 `connect_timeout` 和 `handshake_timeout` 必须是有限且大于零的浮点数。Python 整数字段必须能表示为 Rust `usize`。`None` 只允许用于两项 WebSocket 大小限制，不能用于读取块、超时或并发上限。

## Rust 配置示例

```rust
use std::time::Duration;

use libwsrx::Config;

let config = Config {
    tcp_read_buffer_size: 32 * 1024,
    max_websocket_message_size: Some(8 * 1024 * 1024),
    max_websocket_frame_size: Some(2 * 1024 * 1024),
    connect_timeout: Duration::from_secs(5),
    handshake_timeout: Duration::from_secs(5),
    max_concurrent_tunnels: 256,
};
config.validate()?;
```

`Config::validate()` 适合在应用启动时提前报告错误；即使不显式调用，库的公开入口仍会校验它。

## Python 配置示例

```python
import libwsrx

config = libwsrx.Config(
    tcp_read_buffer_size=32 * 1024,
    max_websocket_message_size=8 * 1024 * 1024,
    max_websocket_frame_size=2 * 1024 * 1024,
    connect_timeout=5.0,
    handshake_timeout=5.0,
    max_concurrent_tunnels=256,
)
```

`Config` 在 Python 中是冻结对象。传入零、负数、无限值、NaN 或超出 `usize` 范围的整数会在构造时抛出 `ValueError`。

## 调优原则

- 将消息和 frame 限制设为部署可承受的上限。取消限制会使对端可声明或发送非常大的 WebSocket 数据，通常只适用于受控网络。
- 增大 `tcp_read_buffer_size` 可能减少小消息开销，但每条活跃隧道的读取操作会分配相应大小的缓冲区。它不是应用协议的消息大小设置。
- `max_concurrent_tunnels` 是背压点：达到上限后监听器暂不接受新连接，因此连接会在操作系统接受队列中等待或被客户端超时。根据文件描述符、内存、目标服务容量和反向代理限制共同设定。
- `connect_timeout` 覆盖拨出 WebSocket 和连接目标 TCP；`handshake_timeout` 只覆盖服务端接收 WebSocket Upgrade。它们不限制已建立隧道的总时长或空闲时长。
- 连接总时长、空闲超时、客户端重试、限流和资源隔离属于应用或代理层策略，不由 `Config` 提供。

生产部署的外部限制与检查项见[部署指南](deployment.md)。
