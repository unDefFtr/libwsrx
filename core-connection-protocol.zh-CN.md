# WSRX 核心连接协议

[README](README.md) · [开发指南](docs/development.md)

本文定义 WSRX 的数据面语义，即 TCP 字节如何经由 WebSocket 传输。Rust/Python API、超时、TLS 支持和具体错误请以 [README](README.md) 为准。

## 一句话说明

WSRX 是一个极简的 TCP-over-WebSocket 协议：它使用标准 WebSocket Binary Message 承载原始 TCP 字节，不在 payload 中增加任何 WSRX 帧或元数据。

```mermaid
flowchart LR
    A["源 TCP 连接"] -->|"TCP 字节流"| B["WSRX 客户端端点"]
    B -->|"WebSocket"| C["WSRX 服务端端点"]
    C -->|"TCP 字节流"| D["目标 TCP 连接"]
```

一条隧道始终对应三条连接关系：

```text
1 条源 TCP 连接 <-> 1 条 WebSocket 连接 <-> 1 条目标 TCP 连接
```

服务可同时处理多条隧道，但它们彼此独立。单条 WebSocket 不共享给多条 TCP 连接，也没有通道 ID。

## 建立连接

WSRX 使用标准 WebSocket Upgrade，不要求额外的应用层握手。客户端取得一条源 TCP 连接后，建立一条 WebSocket；服务端接受 WebSocket 后，为它建立一条目标 TCP 连接。两端都可传输后，该隧道开始代理字节。

协议不规定目标 TCP 服务如何选择，也不在 payload 中传递目标地址、用户信息或访问凭据。这些内容应由 URL、静态配置、反向代理或其他控制面在 WebSocket Upgrade 之前确定。

WSRX 不要求 `Sec-WebSocket-Protocol`。部署方可以自行使用该字段，但这不会改变 WSRX 的数据编码。

## 数据编码

### TCP 到 WebSocket

端点从 TCP 读取一段字节后，将该段直接作为一个 Binary Message 发送：

```text
TCP bytes:          b0 b1 b2 ... bn
Binary payload:     b0 b1 b2 ... bn
```

### WebSocket 到 TCP

端点收到 Binary Message 后，将 payload 中的字节按原有顺序写入另一侧 TCP 流：

```text
Binary payload:     b0 b1 b2 ... bn
TCP bytes:          b0 b1 b2 ... bn
```

payload 中的每一个字节都是 TCP 数据。WSRX 不增加：

- 魔数、版本号、通道 ID 或长度前缀；
- 目标地址、首包元数据或消息类型；
- Base64、序列化、压缩、加密或校验和。

需要保密性和传输完整性时，应使用 TLS；需要业务消息格式时，应由隧道内承载的上层 TCP 协议负责。

## 字节顺序与消息边界

TCP 是字节流，WebSocket 是消息协议。WSRX 保证每个方向的**字节顺序**，但不保证 TCP 读写操作与 WebSocket Message 一一对应。

例如，源端依次写入 `ABC` 和 `DEF`。以下三种发送方式都代表相同的字节流：

```text
Binary("ABCDEF")

Binary("ABC")
Binary("DEF")

Binary("A")
Binary("BCDE")
Binary("F")
```

接收 TCP 应用最终必须得到 `ABCDEF`，但它可能通过一次或多次 `read()` 获得这些字节。WebSocket 的底层分片边界同样不属于协议语义。因此，隧道中的上层协议必须自己定义长度字段、分隔符或其他消息边界。

## 控制消息与全双工

WSRX 使用 Binary Message 传输数据。Text Message 不是可互操作的 TCP 数据载体；Ping、Pong 和 Close 是 WebSocket 控制消息，不写入 TCP 流。

两条传输方向相互独立，可同时工作：

```text
source TCP  -> Binary Message -> target TCP
source TCP  <- Binary Message <- target TCP
```

协议不要求请求和响应交替，也不要求客户端先发送数据。因此，它可以承载由服务端先发送欢迎消息或握手数据的 TCP 协议。实现应遵循底层连接的背压，避免在目标暂时不可写时无限制缓存数据。

## 结束与失败

WSRX 没有专门表达 TCP EOF 或半关闭的应用层消息。TCP EOF、WebSocket Close、底层 I/O 错误、连接失败、超时或本地取消都会结束当前隧道。

当任一方向不能继续传输时，端点应释放另一侧连接并结束整条隧道。重新建立 WebSocket 会产生新隧道，不能延续旧 TCP 会话。

WSRX 不定义应用层错误帧。具体错误码、日志、重试策略和向用户暴露的诊断信息属于端点实现与控制面的职责。

## 不在协议范围内的能力

WSRX 有意不定义以下内容：

- 多路复用：一个 WebSocket 只能承载一个 TCP 流；
- 重连与恢复：没有消息序号、确认偏移或重传机制；
- 服务发现与目标选择：没有目标地址字段或健康检查；
- 身份认证与授权：没有 payload 内凭据或权限模型；
- 端到端处理确认：消息发送成功不代表目标应用已经处理数据。

这些能力可由外围系统实现，但不应修改 Binary payload 的含义。

## 互操作性要求

兼容的客户端和服务端必须满足以下要求：

1. 每条 TCP 连接使用独立的 WebSocket；
2. 仅使用 Binary Message 搬运 TCP 数据；
3. 原样保留每个方向的字节顺序；
4. 不把 WebSocket Message 或分片边界当作业务边界；
5. 正确处理 Ping、Pong 与 Close，且不把它们写入 TCP；
6. 任一连接结束时释放整条隧道；
7. 不要求任何 WSRX 私有帧头或首包元数据。

## 部署安全

协议本身不提供 payload 加密、认证或授权。跨越不可信网络时应使用 `wss://`，并在 Upgrade 前或外围代理中完成身份认证和授权。

目标 TCP 服务必须由可信配置或受控路由决定，避免端点成为可被任意访问的 TCP 转发器。部署层还应限制并发连接、消息大小、连接总时长和空闲时间，并避免把内部地址或网络拓扑直接暴露给客户端。
