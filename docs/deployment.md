# 部署指南

[README](../README.md) · [使用指南](guide.md) · [配置参考](configuration.md) · [核心连接协议](protocol.md)

`libwsrx` 是传输组件，不是认证代理或服务发现系统。生产设计需要在库的监听器外补齐 TLS、身份、访问边界、限流和可观测性。

## 推荐网络模型

在受控内网中，客户端可直接通过 `ws://` 连接服务端监听器。跨越不可信网络时，客户端应连接 `wss://` 入口：

```mermaid
flowchart LR
    A["源 TCP 应用"] --> B["libwsrx 客户端"]
    B -->|"wss://"| P["TLS 终止与访问控制层"]
    P -->|"ws:// 或受控 TLS"| S["libwsrx 服务端"]
    S --> T["固定目标 TCP 服务"]
```

Rust 客户端可以拨出 `ws://` 与 `wss://`。连接 `wss://` 时，它使用系统原生根证书验证服务器证书。高层服务端 API 只接受原始 TCP 连接并完成 WebSocket Upgrade，不能直接提供 TLS 终止；应在其前方部署反向代理、负载均衡器或其他 TLS 终止层。

## Upgrade 转发要求

位于服务端前方的代理必须把 WebSocket Upgrade 作为长连接正确转发，而不能按普通短 HTTP 请求处理。代理配置应保证：

- 客户端能够完成标准 WebSocket Upgrade，并保留需要的 Upgrade/Connection 语义；
- Binary Message 和 Ping/Pong 可双向通过，且不会被转换为 HTTP body 或文本；
- 代理的单条消息/帧、连接数、请求体和长连接超时限制与[配置参考](configuration.md)及目标协议相容；
- 代理的连接、读写和空闲超时足以支撑预期隧道时长；
- 后端只暴露给受控代理或网络边界，避免绕过认证直接访问 WebSocket 服务端。

库不携带 HTTP 认证头、Cookie、路径路由或 `Sec-WebSocket-Protocol` 的业务语义。需要这些策略时，在 Upgrade 前的代理或调用方建立的受控传输中处理。

## 安全边界

`target_addr` 由服务端 API 的参数固定，而不是由客户端在 WebSocket payload 中提交。这避免客户端把库直接用作任意地址的 TCP 代理，但每一个公开入口仍应绑定到明确允许访问的目标服务。

在上线前完成以下控制：

- 在 TLS 终止层或更早位置认证客户端，并按身份授权访问具体 WebSocket 入口；
- 为不同目标、租户或权限边界使用独立服务端入口/配置，避免仅以客户端传入的信息选择目标；
- 允许来自客户端到入口、入口到服务端、服务端到固定目标的最小网络路径；
- 对入口实施连接数、速率、消息大小和资源上限；
- 避免把内部目标地址、网络拓扑或未经处理的错误直接暴露给不可信客户端；
- 用目标服务自身的认证与加密保护端到端业务数据。`wss://` 只保护客户端到 TLS 终止层的传输。

不要依赖库实现认证、授权、重连、空闲超时、可靠重传、服务发现或负载均衡。

## 生产检查清单

- 客户端 URL 使用预期的 `ws://` 或 `wss://`，且 `wss://` 证书链、域名和系统信任根有效。
- 服务端 `target_addr` 指向受控固定目标，目标可从服务端网络连通。
- 外围 TLS/代理正确支持 WebSocket Upgrade、双向 Binary 数据与长连接。
- `Config` 的消息大小、连接/握手超时和并发上限与代理、操作系统和目标服务的限制一致。
- 应用或代理为连接总时长、空闲连接、速率与日志留存设置明确策略。
- 应用已配置 `tracing` 订阅器或其他日志收集方式，以接收每条隧道失败的警告。
- 已通过目标业务协议验证大于一个 TCP 读取块的数据、双向同时传输、连接关闭和故障恢复策略。

## 常见故障

| 现象 | 优先检查 |
| --- | --- |
| 客户端无法建立 `wss://` | URL 主机名、证书 SAN、完整证书链、系统信任根和 TLS 终止层。 |
| WebSocket Upgrade 失败或很快断开 | 代理是否转发 Upgrade，认证是否在 Upgrade 前完成，后端地址是否正确。 |
| Upgrade 成功但隧道立即关闭 | 服务端到固定 `target_addr` 的网络、端口、DNS 与 `connect_timeout`。 |
| 长时间空闲后连接断开 | 代理、负载均衡器、防火墙或 NAT 的空闲超时；库没有空闲保活策略。 |
| 高峰期新连接等待或失败 | `max_concurrent_tunnels`、操作系统接受队列、代理连接上限和目标服务容量。 |
| 大 payload 关闭 | 两端及中间代理的 WebSocket message/frame 限制；库在超出 message 限制时以 Size close 终止隧道。 |
| 对端发送文本后关闭 | WSRX 只接受 Binary Message；改用二进制 payload，不要把文本当作 TCP 数据。 |

协议级关闭规则见[核心连接协议](protocol.md)，库 API 的错误类型见 [Rust API](rust-api.md) 与 [Python API](python-api.md)。
