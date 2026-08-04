# 开发指南

[README](../README.md) · [核心连接协议](../core-connection-protocol.zh-CN.md)

本文说明如何在本地构建、测试和检查 `libwsrx`。除非另有说明，命令都从仓库根目录执行。

## 环境要求

- 支持 Rust 2024 edition 的 Rust/Cargo；
- Python 3.9 或更新版本；
- Maturin `>=1.9.4,<2.0`；
- pytest；
- 可绑定本地回环端口的本地环境。

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

以下命令只生成并校验本地发行包，不会上传：

```console
cargo build --locked --release
maturin build --locked --release --compatibility pypi --out dist
maturin sdist --out dist
python -m twine check --strict dist/*
```

## 发布

唯一发布入口是 GitHub Actions 的 `Publish Release` 工作流。Tag 触发后，工作流会构建并校验所有发行包、根据 `cliff.toml` 生成修改日志、创建 GitHub Release 并发布到 PyPI。不要手工创建 Release，也不要使用 API token 或手工运行 `twine upload`。

首次发布前需要完成以下一次性设置：

1. 在 GitHub 仓库中创建名为 `pypi` 的 Environment；自动发布不需要配置 secret 或必需的 reviewer。
2. 当前 [PyPI JSON API](https://pypi.org/pypi/libwsrx/json) 对 `libwsrx` 返回 404。在 PyPI 账户的 Publishing 页面注册 pending Trusted Publisher，字段固定如下：

   | 字段 | 值 |
   | --- | --- |
   | PyPI project name | `libwsrx` |
   | GitHub owner | `unDefFtr` |
   | GitHub repository name | `libwsrx` |
   | Workflow name | `publish.yml` |
   | Environment name | `pypi` |

首次成功上传后，PyPI 会自动把 pending publisher 转为普通 publisher。如果项目已由当前维护者账户创建，则在项目的 Publishing 设置中添加字段相同的普通 publisher；如果名称已被无关账户占用，停止发布，不要擅自修改包名、导入名或工作流契约。

每次发布按以下顺序操作：

1. 修改 `Cargo.toml` 的 `[package].version`。
2. 运行 `cargo check`，让 Cargo 刷新 `Cargo.lock` 中根包的版本。
3. 运行 `cargo metadata --locked --no-deps`，确认清单与 lockfile 同步，然后提交 `Cargo.toml` 和 `Cargo.lock`。
4. 可先在 GitHub Actions 页面手工运行 `Publish Release`。手工运行只构建、测试和校验发行包，`release` 和 `publish` job 都会跳过，不会创建 GitHub Release 或上传到 PyPI。
5. 准备正式发布时，从 `Cargo.toml` 读取版本并推送唯一标签：

   ```console
   VERSION="$(python -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')"
   git tag "v$VERSION"
   git push origin "v$VERSION"
   ```

工作流要求标签严格等于 `v` 加 Cargo 版本；任一平台构建、wheel 安装测试、源码发行包构建或发行包校验失败都会阻止发布。成功后，GitHub Release 会包含 git-cliff 生成的修改日志以及 5 个 wheel 和 1 个 sdist。PyPI 版本和 Git Tag 均不可覆盖，因此不要重复使用已经发布的版本号。

## 常见问题

- `import libwsrx` 失败：确认已激活预期虚拟环境，然后重新执行 `maturin develop`。
- 端口绑定失败：确认本地回环端口未被占用，且当前环境允许监听本地端口。
- Linux Docker 验证：以下命令使用 `rust:1.91.1-bookworm`、只读 `/work` 绑定挂载和容器内的 `CARGO_TARGET_DIR`，依次运行 `cargo build --locked` 与 `cargo test --locked`，不会修改主机的 `target` 目录：

  ```console
  docker run --rm --platform linux/arm64 -v "$PWD:/work:ro" -w /work -e CARGO_TARGET_DIR=/tmp/libwsrx-target rust:1.91.1-bookworm cargo build --locked
  docker run --rm --platform linux/arm64 -v "$PWD:/work:ro" -w /work -e CARGO_TARGET_DIR=/tmp/libwsrx-target rust:1.91.1-bookworm cargo test --locked
  ```
- `wss://` 证书验证失败：检查证书链、服务器主机名和系统信任根。
- 服务端需要 TLS：高层服务端 API 接受原始 TCP 连接，应在其前方使用反向代理或其他 TLS 终止层。
