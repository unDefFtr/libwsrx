# Repository Guidelines

## Project Overview

`libwsrx` is an embeddable TCP-over-WebSocket tunnel library. It exposes a Rust/Tokio API and an optional Python `asyncio` extension built with PyO3/Maturin; there is no CLI. One source TCP connection maps to one WebSocket connection and one fixed target TCP connection. WebSocket binary messages carry raw TCP bytes; message boundaries are not application framing.

## Architecture & Data Flow

1. `src/client.rs` accepts a local TCP stream and opens a timed outbound `ws://` or `wss://` connection.
2. `src/server.rs` upgrades an inbound WebSocket and opens a timed TCP connection to the configured target.
3. `src/endpoint.rs` runs the shared listener loop, bounds active tunnels with `JoinSet`, handles shutdown, and logs per-tunnel failures without stopping other tunnels.
4. `src/relay.rs` copies bytes in both directions concurrently. Either direction ending terminates the tunnel; text/raw-frame protocol violations close the WebSocket with an appropriate code.
5. `src/python.rs`, behind Cargo feature `python`, adapts Rust futures to Python `asyncio` and maps configuration errors to `ValueError` and operational errors to `WSRXError`.

Configuration and state are explicit and endpoint-local. There is no global mutable state. Dependency-injection seams are generic async transports, pre-bound listeners, `ToSocketAddrs`, and handler closures—not trait-object service containers.

## Key Directories

- `src/`: Rust library implementation and feature-gated Python bindings.
- `tests/`: Rust integration tests (`relay.rs`, `endpoints.rs`) and Python API/lifecycle tests under `tests/python/`.
- `docs/`: maintainer workflow; `docs/development.md` is the effective contribution guide.
- `.github/workflows/`: per-push wheel/sdist build and validation, plus tag-gated GitHub Release and trusted PyPI publishing automation.
- `target/`, `dist/`, `.venv/`: generated local artifacts; keep them untracked.

## Development Commands

Run commands from the repository root.

```bash
cargo build --locked
cargo build --locked --release
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Python binding setup and QA:

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install 'maturin>=1.9.4,<2.0' pytest
maturin develop
python -m pytest tests/python
```

Focused tests:

```bash
cargo test --locked --test relay
cargo test --locked --test endpoints
cargo test --locked --test relay preserves_bytes_across_message_boundaries_in_both_directions
python -m pytest tests/python/test_api.py::test_api_exports_and_config_contract
```

Local packaging uses `maturin build --locked --release --compatibility pypi --out dist`, `maturin sdist --out dist`, and `python -m twine check --strict dist/*`. Twine is not installed by the documented development setup; install it explicitly when packaging.

## Code Conventions & Common Patterns

- Follow idiomatic Rust naming: `snake_case` functions/modules/fields, `PascalCase` types and error variants, and uppercase constants.
- Keep orchestration thin: connection setup belongs in `client.rs`/`server.rs`, listener lifecycle in `endpoint.rs`, and transport semantics in `relay.rs`.
- Every public entry point must call `Config::validate`; preserve this invariant when adding APIs.
- Use the central `Error` enum and `Result` alias from `src/error.rs`. Prefer `?` propagation and phase-specific timeout variants. Ignore cleanup failures only when cleanup is explicitly best-effort.
- Use Tokio primitives consistently: `JoinSet` for bounded connection tasks, `tokio::select!` for competing shutdown/data-flow futures, `tokio::time::timeout` for network phases, and `oneshot` plus `JoinHandle` for managed endpoint shutdown.
- Avoid detached state and global mutation. Move immutable configuration into `'static` tasks; use `Option::take` for resources consumed exactly once.
- Preserve transport generics (`AsyncRead + AsyncWrite + Unpin`) and pre-bound-listener seams instead of introducing unnecessary abstractions.
- Python async wrappers use `pyo3_async_runtimes::tokio::future_into_py`. Keep invalid configuration separate from runtime failure, and preserve `asyncio.CancelledError` behavior.
- Update all affected surfaces together: public Rust API/`Config` changes require README, bindings when applicable, and tests; Python API/exception changes require `src/python.rs`, Python tests, and README; relay/protocol behavior changes require relay and endpoint coverage.

## Important Files

- `src/lib.rs`: Rust crate root and public re-exports.
- `src/client.rs`: client listener, WebSocket dialing, and managed `ClientEndpoint`.
- `src/server.rs`: WebSocket handshake and target TCP connection.
- `src/endpoint.rs`: shared bounded accept loop and shutdown behavior.
- `src/relay.rs`: binary-only bidirectional relay and close semantics.
- `src/config.rs`: public configuration, defaults, and validation.
- `src/error.rs`: central error taxonomy.
- `src/python.rs`: PyO3 module and asyncio wrappers.
- `Cargo.toml` / `Cargo.lock`: Rust package, features, dependencies, and locked versions.
- `pyproject.toml`: Maturin/Python packaging contract.
- `docs/development.md`: canonical development, QA, packaging, and release procedure.
- `.github/workflows/build.yml`: per-push package matrix, installed-wheel tests, distribution validation, and build context.
- `.github/workflows/release.yml`: exact-run artifact consumption, tag/SHA verification, GitHub Release creation, and trusted PyPI publishing.

- `docs/protocol.md`: core connection protocol covering data encoding, message boundaries, and connection lifecycle.

## Runtime/Tooling Preferences

- Rust must support edition 2024; CI uses stable Rust. No exact MSRV or `rust-toolchain` is declared.
- Use Cargo and the committed `Cargo.lock`; normal build, test, lint, and release commands use `--locked`.
- Python support starts at 3.9. Use the active virtual environment’s `python -m pip` and `python -m pytest`.
- Python extensions must be built with Maturin `>=1.9.4,<2.0`; `maturin develop` enables Cargo feature `python`. Plain `cargo build` does not build/install Python bindings.
- Rustfmt and Clippy use defaults; no repository-specific formatter/linter configuration exists.
- AWS-LC builds may require a C/C++ compiler, CMake, and `pkg-config`.
- There is no Make/Just/task runner, Python lockfile, pre-commit setup, or separate `pull_request` workflow. Per-push packaging CI runs through `build.yml`.

## Testing & QA

- Rust tests use the built-in harness plus `#[tokio::test]`; Python tests use pytest discovery with synchronous `test_*` functions calling `asyncio.run`.
- Keep fixtures inline as private helpers. Use in-memory duplex streams or ephemeral `127.0.0.1:0` listeners; tests must not require external services.
- Bound network and concurrency waits with timeouts. Explicitly close, cancel, abort, and await spawned resources during teardown.
- Assert observable behavior: exact bytes, endpoint isolation, EOF/closure, cancellation, and precise error variants.
- Minimum Rust pre-submit QA: fmt check, Clippy with warnings denied, and `cargo test --locked`. Binding changes also require `maturin develop` and `python -m pytest tests/python`.
- No coverage tool, threshold, or repository coverage policy is configured. Add behavior-focused tests for new observable contracts rather than source-structure assertions.

## Version Control & Commits

- Commit every modification. Do not leave completed changes uncommitted at handoff.
- Use the repository’s observed concise Conventional Commit style: `feat: ...`, `fix: ...`, `test: ...`, `docs: ...`, `refactor: ...`, `chore: ...`, or `ci: ...`.
- Keep commits focused and include only files changed for that modification; never absorb unrelated working-tree changes.
- For a release version bump, update `Cargo.toml`, run `cargo check` to refresh `Cargo.lock`, verify with `cargo metadata --locked --no-deps`, and commit both files together.
- Release tags must be unique and exactly `v<version>` matching `Cargo.toml`. Publish only through `.github/workflows/release.yml`, after its triggering `.github/workflows/build.yml` run succeeds; do not manually create/run a Release or upload with Twine or an API token.
