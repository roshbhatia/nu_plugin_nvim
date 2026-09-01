use std::collections::VecDeque;
use std::io::{BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use rmpv::Value;
use thiserror::Error;

use crate::ServerAddress;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

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
                let stream = UnixStream::connect(path).map_err(|source| RpcError::Connect {
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
                let socket = address
                    .to_socket_addrs()
                    .map_err(|source| RpcError::Resolve {
                        server: server.clone(),
                        source,
                    })?
                    .next()
                    .ok_or_else(|| RpcError::NoResolvedAddress(server.clone()))?;
                let stream = TcpStream::connect_timeout(&socket, timeout).map_err(|source| {
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
            .ok_or(RpcError::RequestIdOverflow)?;
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
            match parse_message(&message)? {
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
}

enum Incoming {
    Response {
        id: u64,
        error: Value,
        result: Value,
    },
    Notification(Notification),
}

fn parse_message(message: &Value) -> Result<Incoming, RpcError> {
    let items = message
        .as_array()
        .ok_or_else(|| RpcError::Malformed("RPC message is not an array".into()))?;
    let message_type = items
        .first()
        .and_then(Value::as_u64)
        .ok_or_else(|| RpcError::Malformed("RPC message has no numeric type".into()))?;
    match message_type {
        1 if items.len() == 4 => Ok(Incoming::Response {
            id: items[1].as_u64().ok_or_else(|| {
                RpcError::Malformed("RPC response has no numeric request id".into())
            })?,
            error: items[2].clone(),
            result: items[3].clone(),
        }),
        2 if items.len() == 3 => Ok(Incoming::Notification(Notification {
            method: items[1]
                .as_str()
                .ok_or_else(|| RpcError::Malformed("RPC notification has no method string".into()))?
                .into(),
            arguments: items[2]
                .as_array()
                .ok_or_else(|| {
                    RpcError::Malformed("RPC notification arguments are not an array".into())
                })?
                .clone(),
        })),
        1 => Err(RpcError::Malformed(format!(
            "RPC response has {} fields, expected 4",
            items.len()
        ))),
        2 => Err(RpcError::Malformed(format!(
            "RPC notification has {} fields, expected 3",
            items.len()
        ))),
        other => Err(RpcError::Malformed(format!(
            "unsupported RPC message type {other}"
        ))),
    }
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
    #[error("malformed Neovim RPC response: {0}")]
    Malformed(String),
    #[error("Neovim RPC request identifier overflowed")]
    RequestIdOverflow,
}
