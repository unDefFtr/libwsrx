# 开发指南

[README](../README.md) · [核心连接协议](../core-connection-protocol.zh-CN.md)

本文说明如何在本地构建、测试和检查 `libwsrx`。除非另有说明，命令都从仓库根目录执行。

## 环境要求

- 支持 Rust 2024 edition 的 Rust/Cargo；
- Python 3.9 或更新版本；
- Maturin `>=1.9.4,<2.0`；
- pytest；
- 可绑定本地回环端口的本地环境。

依赖中的 AWS-LC 可能需要可用的 C/C++ 编译器、CMake 和 `pkg-config`。具体安装命令取决于操作系统。

## 准备 Python 环境

建议在虚拟环境中构建 Python 扩展：

```console
python3 -m venv .venv
. .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2.0' pytest
```

在 PowerShell 中激活环境：

```powershell
.venv\Scripts\Activate.ps1
```

`maturin develop` 会构建扩展，并安装到当前激活的 Python 解释器。它会启用 Cargo 的 `python` 功能开关；单独运行 `cargo build` 不会构建 Python 绑定。

## 构建

构建 Rust 库：

```console
cargo build --locked
```

构建优化版本：

```console
cargo build --locked --release
```

构建并安装 Python 扩展到当前虚拟环境：

```console
maturin develop
```

## 测试

运行完整 Rust 测试集：

```console
cargo test --locked
```

只运行某一组测试：

```console
cargo test --locked --test relay
cargo test --locked --test endpoints
```

`tests/relay.rs` 使用内存连接，验证字节顺序、全双工传输和 WebSocket 消息处理。`tests/endpoints.rs` 使用本地回环连接，验证端点并发、隔离、关闭、超时和客户端 TLS 握手。

运行 Python 测试前，先执行 `maturin develop`，随后运行：

```console
python -m pytest tests/python
```

Python 测试覆盖导出 API、配置校验和 asyncio 生命周期。全部测试均不依赖外部网络服务。

## 提交前检查

修改 Rust 代码后，至少运行：

```console
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

修改 Python 绑定时，还应执行：

```console
maturin develop
python -m pytest tests/python
```

## 代码结构

| 路径 | 职责 |
| --- | --- |
| `src/client.rs` | 接受本地 TCP 连接，并为每条连接建立出站 WebSocket。 |
| `src/server.rs` | 接受承载 WebSocket 的 TCP 连接，并连接固定目标 TCP 服务。 |
| `src/relay.rs` | 在 TCP 和 WebSocket 之间双向转发字节，并定义关闭行为。 |
| `src/endpoint.rs` | 管理监听器、并发上限、取消和单条隧道隔离。 |
| `src/config.rs` / `src/error.rs` | Rust 与 Python 共用的配置和错误契约。 |
| `src/python.rs` | 将 Rust API 暴露给 asyncio，并转换配置和运行时错误。 |
| `tests/relay.rs` | 数据面集成测试。 |
| `tests/endpoints.rs` | Rust 端点集成测试。 |
| `tests/python/test_api.py` | Python API 和 asyncio 集成测试。 |

运行时路径为：客户端接受源 TCP，建立出站 WebSocket；服务端完成 Upgrade 并连接固定目标；`relay` 在两个方向并发转发字节。任一方向结束后，整条隧道结束。单条隧道失败会记录警告，不会停止其他连接；监听器本身失败则会结束端点。

## 修改时需要同步的内容

| 修改范围 | 同步更新 |
| --- | --- |
| Rust 公共 API 或 `Config` | README、Python 绑定（如适用）和对应测试。 |
| Python API 或异常行为 | `src/python.rs`、`tests/python/test_api.py` 和 README。 |
| 数据编码、消息处理或关闭语义 | `tests/relay.rs`、`tests/endpoints.rs` 和 [核心连接协议](../core-connection-protocol.zh-CN.md)。 |
| TLS、超时或连接行为 | 端点测试和 README 的部署说明。 |

所有公共入口都会校验 `Config`。新增入口必须保持这一约束。

## 本地打包

以下命令只生成本地构建产物，不会发布：

```console
cargo build --locked --release
maturin build --release --out dist
```

## 常见问题

- `import libwsrx` 失败：确认已激活预期虚拟环境，然后重新执行 `maturin develop`。
- 端口绑定失败：确认本地回环端口未被占用，且当前环境允许监听本地端口。
- AWS-LC 构建失败：确认原生编译器、CMake 和 `pkg-config` 可供 Cargo 使用。
- `wss://` 证书验证失败：检查证书链、服务器主机名和系统信任根。
- 服务端需要 TLS：高层服务端 API 接受原始 TCP 连接，应在其前方使用反向代理或其他 TLS 终止层。
