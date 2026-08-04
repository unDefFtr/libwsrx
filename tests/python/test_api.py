import asyncio
import math
import socket
import time

import libwsrx


BANNER = b"BANNER\n"


def assert_value_error(**kwargs):
    try:
        libwsrx.Config(**kwargs)
    except ValueError:
        return
    raise AssertionError(f"Config accepted invalid arguments: {kwargs!r}")


def test_api_exports_and_config_contract():
    assert {"Config", "WSRXError", "run_client", "run_server"} <= set(dir(libwsrx))
    assert issubclass(libwsrx.WSRXError, Exception)

    config = libwsrx.Config()
    assert config.tcp_read_buffer_size == 65_536
    assert config.max_websocket_message_size == 67_108_864
    assert config.max_websocket_frame_size == 16_777_216
    assert config.connect_timeout == 10.0
    assert config.handshake_timeout == 10.0
    assert config.max_concurrent_tunnels == 1_024

    unlimited = libwsrx.Config(
        max_websocket_message_size=None,
        max_websocket_frame_size=None,
    )
    assert unlimited.max_websocket_message_size is None
    assert unlimited.max_websocket_frame_size is None

    for field in (
        "tcp_read_buffer_size",
        "max_websocket_message_size",
        "max_websocket_frame_size",
        "max_concurrent_tunnels",
    ):
        assert_value_error(**{field: 0})
    for field in ("connect_timeout", "handshake_timeout"):
        for value in (0.0, -1.0, math.inf, -math.inf, math.nan):
            assert_value_error(**{field: value})

    try:
        config.tcp_read_buffer_size = 1
    except AttributeError:
        pass
    else:
        raise AssertionError("Config must be frozen")


def reserve_loopback_port():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


async def wait_for_banner(local_port):
    deadline = time.monotonic() + 5.0
    last_error = None
    while time.monotonic() < deadline:
        writer = None
        try:
            reader, writer = await asyncio.open_connection("127.0.0.1", local_port)
            banner = await asyncio.wait_for(reader.readexactly(len(BANNER)), 0.5)
            if banner == BANNER:
                return reader, writer
        except (ConnectionError, asyncio.IncompleteReadError, asyncio.TimeoutError) as error:
            last_error = error
        if writer is not None:
            writer.close()
            await writer.wait_closed()
        await asyncio.sleep(0.02)
    raise AssertionError(f"endpoints did not become ready: {last_error!r}")


async def cancel_and_assert(future):
    future.cancel()
    try:
        await future
    except asyncio.CancelledError:
        return
    raise AssertionError("endpoint future did not raise CancelledError")


async def exercise_asyncio_endpoints():
    async def echo_with_banner(reader, writer):
        try:
            writer.write(BANNER)
            await writer.drain()
            while chunk := await reader.read(32 * 1024):
                writer.write(chunk)
                await writer.drain()
        finally:
            writer.close()
            await writer.wait_closed()

    target = await asyncio.start_server(echo_with_banner, "127.0.0.1", 0)
    target_port = target.sockets[0].getsockname()[1]
    websocket_port = reserve_loopback_port()
    local_port = reserve_loopback_port()

    server_future = asyncio.ensure_future(
        libwsrx.run_server(
            f"127.0.0.1:{websocket_port}",
            f"127.0.0.1:{target_port}",
        )
    )
    client_future = asyncio.ensure_future(
        libwsrx.run_client(
            f"127.0.0.1:{local_port}",
            f"ws://127.0.0.1:{websocket_port}",
        )
    )

    writer = None
    try:
        reader, writer = await wait_for_banner(local_port)
        payload = bytes(range(256)) * 300 + b"\x00\xff\xfeEND"
        assert len(payload) > 65_536
        writer.write(payload)
        await writer.drain()
        echoed = await asyncio.wait_for(reader.readexactly(len(payload)), 2.0)
        assert echoed == payload

        await asyncio.gather(
            cancel_and_assert(client_future),
            cancel_and_assert(server_future),
        )
        assert await asyncio.wait_for(reader.read(1), 1.0) == b""
    finally:
        for future in (client_future, server_future):
            if not future.done():
                future.cancel()
        await asyncio.gather(client_future, server_future, return_exceptions=True)
        if writer is not None:
            writer.close()
            await writer.wait_closed()
        target.close()
        await target.wait_closed()


def test_asyncio_client_and_server_end_to_end():
    asyncio.run(exercise_asyncio_endpoints())

