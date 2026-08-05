# libwsrx

`libwsrx` 是一个面向嵌入式使用的 TCP-over-WebSocket 隧道库，提供 Rust 库和 Python asyncio 扩展。它把本地 TCP 连接通过 WebSocket 转发到固定的目标 TCP 服务。

> 这不是命令行工具，也不定义用户、路由或服务发现。把它当作应用或网关中的一个传输组件。

## 它解决什么问题

当 TCP 客户端无法直接抵达目标服务、但可以抵达 WebSocket 入口时，可部署两端：

```mermaid
flowchart LR
    A["TCP 客户端"] -->|"本地 TCP"| B["libwsrx 客户端端点"]
    B -->|"WebSocket"| C["libwsrx 服务端端点"]
    C -->|"TCP"| D["固定目标服务"]
```

每一条源 TCP 连接都会创建一条独立的 WebSocket，并在服务端创建一条独立的目标 TCP 连接：

```text
1 TCP connection <-> 1 WebSocket connection <-> 1 TCP connection
```

隧道是全双工的，并保证同一方向的字节顺序。它只传递 WebSocket Binary Message 中的原始 TCP 字节，不添加私有帧头。WebSocket 消息边界没有业务含义，因此上层 TCP 协议仍需自行定义消息边界。

## 适用范围

已提供：

- `ws://` 传输；Rust 客户端也支持 `wss://`；
- 多条独立隧道的并发处理；
- 读块、WebSocket 消息与帧大小、连接和握手超时、并发上限配置；
- Rust Tokio API 与 Python asyncio API。

未提供：

- 多路复用、通道 ID 或自定义 WSRX framing；
- 身份认证、授权、目标发现或负载均衡；
- 自动重连、可靠重传、会话恢复、空闲超时；
- TCP half-close 的映射；
- CLI，或服务端内置 TLS 终止。

## 安装

### Rust

在调用方的 `Cargo.toml` 中使用路径依赖：

```toml
[dependencies]
libwsrx = { path = "../libwsrx" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Rust crate 目前不发布到 crates.io。

### Python

项目要求 Python 3.9 或更新版本：

```console
python -m pip install libwsrx
```

这会安装预构建的原生扩展；如果当前平台没有可用的 wheel，pip 会从源码构建。从当前源码准备开发环境，见[开发指南](docs/development.md)。

## 五分钟验证

以下示例将本机的 HTTP 服务固定为目标。它仅用于本地回环验证，不包含认证或 TLS。

先启动目标服务：

```console
python3 -m http.server 9100 --bind 127.0.0.1
```

然后运行下面的 Python 程序。它在 `9000` 启动 WebSocket 服务端，在随机本地端口启动 TCP 客户端端点。

```python
import asyncio

import libwsrx


async def main():
    server = asyncio.create_task(
        libwsrx.run_server("127.0.0.1:9000", "127.0.0.1:9100")
    )
    endpoint = await libwsrx.ClientEndpoint.bind(
        "127.0.0.1:0", "ws://127.0.0.1:9000"
    )
    try:
        print(endpoint.local_addr)
        await asyncio.Event().wait()
    finally:
        await endpoint.shutdown()
        server.cancel()
        await asyncio.gather(server, return_exceptions=True)


try:
    asyncio.run(main())
except KeyboardInterrupt:
    pass
```

程序打印出例如 `127.0.0.1:54321` 的地址后，在另一个终端访问它：

```console
curl --fail http://127.0.0.1:54321/
```

将示例地址替换为实际打印的地址。响应由 `9100` 的 HTTP 服务返回，但请求路径是本地 TCP -> WebSocket -> 目标 TCP。按 `Ctrl-C` 停止程序和临时 HTTP 服务。

## 用户文档

- [使用指南](docs/guide.md)：架构、数据流、嵌入方式与生命周期。
- [部署指南](docs/deployment.md)：TLS、反向代理、安全边界、生产检查与排错。
- [配置参考](docs/configuration.md)：所有限制、超时和调优建议。
- [Rust API 参考](docs/rust-api.md)：全部公开 Rust API、资源所有权与错误。
- [Python API 参考](docs/python-api.md)：asyncio API、关闭和异常契约。
- [核心连接协议](docs/protocol.md)：Binary Message 数据面与互操作性规范。

维护、构建、测试与发布信息见[开发指南](docs/development.md)。

## 许可证

本项目使用 [MIT 许可证](LICENSE)。
