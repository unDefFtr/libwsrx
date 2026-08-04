use std::time::Duration;

use pyo3::{
    Bound, PyErr, PyResult, Python, create_exception,
    exceptions::{PyException, PyValueError},
    prelude::*,
    types::{PyAny, PyModule},
    wrap_pyfunction,
};

use crate::{Config, Error, client, server};

create_exception!(libwsrx, WSRXError, PyException);

#[pyclass(name = "Config", frozen)]
struct PyConfig {
    inner: Config,
}

#[pymethods]
impl PyConfig {
    #[new]
    #[pyo3(signature = (
        *,
        tcp_read_buffer_size = 65_536,
        max_websocket_message_size = Some(67_108_864),
        max_websocket_frame_size = Some(16_777_216),
        connect_timeout = 10.0,
        handshake_timeout = 10.0,
        max_concurrent_tunnels = 1_024,
    ))]
    fn new(
        tcp_read_buffer_size: usize,
        max_websocket_message_size: Option<usize>,
        max_websocket_frame_size: Option<usize>,
        connect_timeout: f64,
        handshake_timeout: f64,
        max_concurrent_tunnels: usize,
    ) -> PyResult<Self> {
        let inner = Config {
            tcp_read_buffer_size,
            max_websocket_message_size,
            max_websocket_frame_size,
            connect_timeout: parse_duration("connect_timeout", connect_timeout)?,
            handshake_timeout: parse_duration("handshake_timeout", handshake_timeout)?,
            max_concurrent_tunnels,
        };
        inner.validate().map_err(invalid_config)?;
        Ok(Self { inner })
    }

    #[getter]
    fn tcp_read_buffer_size(&self) -> usize {
        self.inner.tcp_read_buffer_size
    }

    #[getter]
    fn max_websocket_message_size(&self) -> Option<usize> {
        self.inner.max_websocket_message_size
    }

    #[getter]
    fn max_websocket_frame_size(&self) -> Option<usize> {
        self.inner.max_websocket_frame_size
    }

    #[getter]
    fn connect_timeout(&self) -> f64 {
        self.inner.connect_timeout.as_secs_f64()
    }

    #[getter]
    fn handshake_timeout(&self) -> f64 {
        self.inner.handshake_timeout.as_secs_f64()
    }

    #[getter]
    fn max_concurrent_tunnels(&self) -> usize {
        self.inner.max_concurrent_tunnels
    }
}

#[pyfunction]
#[pyo3(signature = (local_addr, websocket_url, *, config = None))]
fn run_client<'py>(
    py: Python<'py>,
    local_addr: String,
    websocket_url: String,
    config: Option<PyRef<'_, PyConfig>>,
) -> PyResult<Bound<'py, PyAny>> {
    let config = config
        .map(|config| config.inner.clone())
        .unwrap_or_default();

    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        client::bind_and_serve(local_addr, websocket_url, config)
            .await
            .map_err(runtime_error)
    })
}

#[pyfunction]
#[pyo3(signature = (websocket_addr, target_addr, *, config = None))]
fn run_server<'py>(
    py: Python<'py>,
    websocket_addr: String,
    target_addr: String,
    config: Option<PyRef<'_, PyConfig>>,
) -> PyResult<Bound<'py, PyAny>> {
    let config = config
        .map(|config| config.inner.clone())
        .unwrap_or_default();

    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        server::bind_and_serve(websocket_addr, target_addr, config)
            .await
            .map_err(runtime_error)
    })
}

#[pymodule]
fn libwsrx(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyConfig>()?;
    module.add("WSRXError", module.py().get_type::<WSRXError>())?;
    module.add_function(wrap_pyfunction!(run_client, module)?)?;
    module.add_function(wrap_pyfunction!(run_server, module)?)?;
    Ok(())
}

fn parse_duration(field: &'static str, seconds: f64) -> PyResult<Duration> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(PyValueError::new_err(format!(
            "{field} must be finite and greater than zero"
        )));
    }

    Duration::try_from_secs_f64(seconds).map_err(|_| {
        PyValueError::new_err(format!("{field} is too large to represent as a duration"))
    })
}

fn invalid_config(error: Error) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn runtime_error(error: Error) -> PyErr {
    WSRXError::new_err(error.to_string())
}
