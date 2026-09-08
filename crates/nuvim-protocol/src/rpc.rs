use std::collections::VecDeque;
use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use rmpv::Value;
use thiserror::Error;

use crate::ServerAddress;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

trait ReadWrite: Read + Write + Send {
    fn prepare_notifications(&self) -> std::io::Result<Box<dyn FnOnce() + Send>>;
}

impl ReadWrite for UnixStream {
    fn prepare_notifications(&self) -> std::io::Result<Box<dyn FnOnce() + Send>> {
        let interrupt = self.try_clone()?;
        self.set_read_timeout(None)?;
        Ok(Box::new(move || {
            let _ = interrupt.shutdown(Shutdown::Both);
        }))
    }
}

impl ReadWrite for TcpStream {
    fn prepare_notifications(&self) -> std::io::Result<Box<dyn FnOnce() + Send>> {
        let interrupt = self.try_clone()?;
        self.set_read_timeout(None)?;
        Ok(Box::new(move || {
            let _ = interrupt.shutdown(Shutdown::Both);
        }))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub method: String,
    pub arguments: Vec<Value>,
}

pub struct RpcClient {
    stream: BufReader<Box<dyn ReadWrite>>,
    next_id: u64,
    server: String,
    notifications: VecDeque<Notification>,
}

impl RpcClient {
    /// # Errors
    /// Returns an error when the client cannot connect or configure timeouts.
    pub fn connect(address: &ServerAddress) -> Result<Self, RpcError> {
        Self::connect_with_timeout(address, DEFAULT_TIMEOUT)
    }

    /// # Errors
    /// Returns an error when the client cannot connect or configure timeouts.
    pub fn connect_with_timeout(
        address: &ServerAddress,
        timeout: Duration,
    ) -> Result<Self, RpcError> {
        let server = address.to_string();
        let stream: Box<dyn ReadWrite> = match address {
            ServerAddress::Unix(path) => {
                let path = path.clone();
                let stream = run_io_with_timeout("Unix socket connection", timeout, move || {
                    UnixStream::connect(path)
                })
                .map_err(|source| RpcError::Connect {
                    server: server.clone(),
                    source,
                })?;
                stream
                    .set_read_timeout(Some(timeout))
                    .map_err(|source| RpcError::Configure {
                        server: server.clone(),
                        source,
                    })?;
                stream
                    .set_write_timeout(Some(timeout))
                    .map_err(|source| RpcError::Configure {
                        server: server.clone(),
                        source,
                    })?;
                Box::new(stream)
            }
            ServerAddress::Tcp(address) => {
                let started = Instant::now();
                let address = address.clone();
                let sockets = run_io_with_timeout("TCP address resolution", timeout, move || {
                    address
                        .to_socket_addrs()
                        .map(std::iter::Iterator::collect::<Vec<_>>)
                })
                .map_err(|source| RpcError::Resolve {
                    server: server.clone(),
                    source,
                })?;
                let socket = sockets
                    .into_iter()
                    .next()
                    .ok_or_else(|| RpcError::NoResolvedAddress(server.clone()))?;
                let remaining =
                    timeout
                        .checked_sub(started.elapsed())
                        .ok_or_else(|| RpcError::Connect {
                            server: server.clone(),
                            source: timeout_error("TCP connection", timeout),
                        })?;
                let stream = TcpStream::connect_timeout(&socket, remaining).map_err(|source| {
                    RpcError::Connect {
                        server: server.clone(),
                        source,
                    }
                })?;
                stream
                    .set_read_timeout(Some(timeout))
                    .map_err(|source| RpcError::Configure {
                        server: server.clone(),
                        source,
                    })?;
                stream
                    .set_write_timeout(Some(timeout))
                    .map_err(|source| RpcError::Configure {
                        server: server.clone(),
                        source,
                    })?;
                Box::new(stream)
            }
        };
        Ok(Self {
            stream: BufReader::new(stream),
            next_id: 1,
            server,
            notifications: VecDeque::new(),
        })
    }

    #[must_use]
    pub fn server(&self) -> &str {
        &self.server
    }

    /// # Errors
    /// Returns an error for transport failures, malformed responses, or remote errors.
    pub fn call(&mut self, method: &str, arguments: Vec<Value>) -> Result<Value, RpcError> {
        let request_id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| RpcError::RequestIdOverflow {
                server: self.server.clone(),
            })?;
        let request = Value::Array(vec![
            Value::from(0),
            Value::from(request_id),
            Value::from(method),
            Value::Array(arguments),
        ]);
        rmpv::encode::write_value(self.stream.get_mut(), &request).map_err(|source| {
            RpcError::Encode {
                method: method.into(),
                server: self.server.clone(),
                source,
            }
        })?;
        self.stream
            .get_mut()
            .flush()
            .map_err(|source| RpcError::Write {
                method: method.into(),
                server: self.server.clone(),
                source,
            })?;

        loop {
            let message =
                rmpv::decode::read_value(&mut self.stream).map_err(|source| RpcError::Decode {
                    method: method.into(),
                    server: self.server.clone(),
                    source,
                })?;
            match parse_message(&message, &self.server)? {
                Incoming::Response { id, error, result } => {
                    if id != request_id {
                        return Err(RpcError::UnexpectedResponseId {
                            method: method.into(),
                            server: self.server.clone(),
                            expected: request_id,
                            actual: id,
                        });
                    }
                    if !error.is_nil() {
                        return Err(RpcError::Remote {
                            method: method.into(),
                            server: self.server.clone(),
                            detail: format_value(&error),
                        });
                    }
                    return Ok(result);
                }
                Incoming::Notification(notification) => self.notifications.push_back(notification),
            }
        }
    }

    pub fn pop_notification(&mut self) -> Option<Notification> {
        self.notifications.pop_front()
    }

    /// # Errors
    /// Returns an error if the socket cannot enter interruptible streaming mode.
    pub fn prepare_notifications(&self) -> Result<Box<dyn FnOnce() + Send>, RpcError> {
        self.stream
            .get_ref()
            .prepare_notifications()
            .map_err(|source| RpcError::Configure {
                server: self.server.clone(),
                source,
            })
    }

    /// # Errors
    /// Returns an error for disconnected, malformed, or unexpected RPC messages.
    pub fn next_notification(&mut self) -> Result<Notification, RpcError> {
        if let Some(notification) = self.pop_notification() {
            return Ok(notification);
        }
        let message =
            rmpv::decode::read_value(&mut self.stream).map_err(|source| RpcError::Decode {
                method: "notification stream".into(),
                server: self.server.clone(),
                source,
            })?;
        match parse_message(&message, &self.server)? {
            Incoming::Notification(notification) => Ok(notification),
            Incoming::Response { .. } => Err(RpcError::malformed(
                &self.server,
                "unexpected response in notification stream",
            )),
        }
    }
}

enum Incoming {
    Response {
        id: u64,
        error: Value,
        result: Value,
    },
    Notification(Notification),
}

fn parse_message(message: &Value, server: &str) -> Result<Incoming, RpcError> {
    let items = message
        .as_array()
        .ok_or_else(|| RpcError::malformed(server, "RPC message is not an array"))?;
    let message_type = items
        .first()
        .and_then(Value::as_u64)
        .ok_or_else(|| RpcError::malformed(server, "RPC message has no numeric type"))?;
    match message_type {
        1 if items.len() == 4 => Ok(Incoming::Response {
            id: items[1].as_u64().ok_or_else(|| {
                RpcError::malformed(server, "RPC response has no numeric request id")
            })?,
            error: items[2].clone(),
            result: items[3].clone(),
        }),
        2 if items.len() == 3 => Ok(Incoming::Notification(Notification {
            method: items[1]
                .as_str()
                .ok_or_else(|| {
                    RpcError::malformed(server, "RPC notification has no method string")
                })?
                .into(),
            arguments: items[2]
                .as_array()
                .ok_or_else(|| {
                    RpcError::malformed(server, "RPC notification arguments are not an array")
                })?
                .clone(),
        })),
        1 => Err(RpcError::malformed(
            server,
            format!("RPC response has {} fields, expected 4", items.len()),
        )),
        2 => Err(RpcError::malformed(
            server,
            format!("RPC notification has {} fields, expected 3", items.len()),
        )),
        other => Err(RpcError::malformed(
            server,
            format!("unsupported RPC message type {other}"),
        )),
    }
}

fn run_io_with_timeout<T: Send + 'static>(
    operation: &'static str,
    timeout: Duration,
    run: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(format!("nuvim-{}", operation.replace(' ', "-")))
        .spawn(move || {
            let _ = sender.send(run());
        })?;
    receiver
        .recv_timeout(timeout)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => timeout_error(operation, timeout),
            mpsc::RecvTimeoutError::Disconnected => std::io::Error::other(format!(
                "{operation} worker stopped without returning a result"
            )),
        })?
}

fn timeout_error(operation: &str, timeout: Duration) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("{operation} exceeded {timeout:?}"),
    )
}

fn format_value(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| format!("{value:?}"), str::to_owned)
}

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("could not connect to Neovim server {server}: {source}")]
    Connect {
        server: String,
        source: std::io::Error,
    },
    #[error("could not resolve Neovim server {server}: {source}")]
    Resolve {
        server: String,
        source: std::io::Error,
    },
    #[error("Neovim server {0} resolved to no addresses")]
    NoResolvedAddress(String),
    #[error("could not configure Neovim server {server}: {source}")]
    Configure {
        server: String,
        source: std::io::Error,
    },
    #[error("could not encode RPC method {method} for Neovim server {server}: {source}")]
    Encode {
        method: String,
        server: String,
        source: rmpv::encode::Error,
    },
    #[error("could not write RPC method {method} to Neovim server {server}: {source}")]
    Write {
        method: String,
        server: String,
        source: std::io::Error,
    },
    #[error("could not decode RPC method {method} from Neovim server {server}: {source}")]
    Decode {
        method: String,
        server: String,
        source: rmpv::decode::Error,
    },
    #[error("Neovim RPC method {method} failed on server {server}: {detail}")]
    Remote {
        method: String,
        server: String,
        detail: String,
    },
    #[error(
        "Neovim RPC method {method} on server {server} returned response {actual}, expected {expected}"
    )]
    UnexpectedResponseId {
        method: String,
        server: String,
        expected: u64,
        actual: u64,
    },
    #[error("malformed Neovim RPC response from server {server}: {detail}")]
    Malformed { server: String, detail: String },
    #[error("Neovim RPC request identifier overflowed for server {server}")]
    RequestIdOverflow { server: String },
}

impl RpcError {
    fn malformed(server: &str, detail: impl Into<String>) -> Self {
        Self::Malformed {
            server: server.to_owned(),
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::net::UnixListener;
    use std::thread;
    use std::time::{Duration, Instant};

    use rmpv::Value;
    use tempfile::tempdir;

    use super::{RpcClient, RpcError, run_io_with_timeout};
    use crate::ServerAddress;

    #[test]
    fn streaming_preserves_partial_frames_across_idle_periods() {
        let directory = tempdir().expect("temporary directory should exist");
        let socket = directory.path().join("partial.sock");
        let listener = UnixListener::bind(&socket).expect("socket should bind");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("client should connect");
            let mut bytes = Vec::new();
            rmpv::encode::write_value(
                &mut bytes,
                &Value::Array(vec![
                    Value::from(2),
                    Value::from("event"),
                    Value::Array(vec![Value::from("payload")]),
                ]),
            )
            .expect("frame should encode");
            stream.write_all(&bytes[..3]).expect("prefix should write");
            thread::sleep(Duration::from_millis(150));
            stream.write_all(&bytes[3..]).expect("suffix should write");
        });
        let mut client = RpcClient::connect_with_timeout(
            &ServerAddress::Unix(socket),
            Duration::from_millis(50),
        )
        .expect("client should connect");
        let interrupt = client
            .prepare_notifications()
            .expect("stream should configure");
        let event = client
            .next_notification()
            .expect("partial frame should remain valid");
        assert_eq!(event.arguments, vec![Value::from("payload")]);
        interrupt();
        server.join().expect("server should stop");
    }

    #[test]
    fn shutdown_interrupts_an_idle_notification_reader() {
        let directory = tempdir().expect("temporary directory should exist");
        let socket = directory.path().join("idle.sock");
        let listener = UnixListener::bind(&socket).expect("socket should bind");
        let mut client =
            RpcClient::connect(&ServerAddress::Unix(socket)).expect("client should connect");
        let (_peer, _) = listener.accept().expect("peer should accept");
        let interrupt = client
            .prepare_notifications()
            .expect("stream should configure");
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = thread::spawn(move || {
            sender
                .send(client.next_notification())
                .expect("receiver should exist");
        });
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        interrupt();
        assert!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("reader should unblock")
                .is_err()
        );
        reader.join().expect("reader should stop");
    }

    #[test]
    fn call_times_out_with_method_and_server_context() {
        let directory = tempdir().expect("temporary directory should exist");
        let socket = directory.path().join("timeout.sock");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("client should connect");
            thread::sleep(Duration::from_millis(250));
        });
        let address = ServerAddress::Unix(socket.clone());
        let mut client = RpcClient::connect_with_timeout(&address, Duration::from_millis(50))
            .expect("client should connect");
        let started = Instant::now();

        let error = client
            .call("nvim_get_mode", vec![])
            .expect_err("silent peer should time out");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            error,
            RpcError::Decode {
                ref method,
                ref server,
                ..
            } if method == "nvim_get_mode" && server == &socket.display().to_string()
        ));
        server.join().expect("test server should stop");
    }

    #[test]
    fn malformed_reply_is_rejected_without_a_panic() {
        let directory = tempdir().expect("temporary directory should exist");
        let socket = directory.path().join("malformed.sock");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("client should connect");
            rmpv::decode::read_value(&mut stream).expect("client request should decode");
            rmpv::encode::write_value(&mut stream, &Value::from("not an RPC array"))
                .expect("reply should encode");
            stream.flush().expect("reply should flush");
        });
        let address = ServerAddress::Unix(socket);
        let mut client = RpcClient::connect_with_timeout(&address, Duration::from_secs(1))
            .expect("client should connect");

        let error = client
            .call("nvim_get_mode", vec![])
            .expect_err("malformed reply should fail");

        assert!(matches!(
            error,
            RpcError::Malformed {
                ref server,
                ref detail,
            } if server == &address.to_string() && detail.contains("not an array")
        ));
        server.join().expect("test server should stop");
    }

    #[test]
    fn blocking_connection_work_is_bounded() {
        let timeout = Duration::from_millis(25);
        let started = Instant::now();

        let error = run_io_with_timeout("test connection", timeout, || {
            thread::sleep(Duration::from_millis(250));
            Ok(())
        })
        .expect_err("blocking connection work should time out");

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(200));
    }
}
