use std::env;
use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerAddress {
    Unix(PathBuf),
    Tcp(String),
}

impl ServerAddress {
    /// # Errors
    /// Returns an error when the address is empty.
    pub fn parse(value: impl Into<String>) -> Result<Self, ServerDiscoveryError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ServerDiscoveryError::Empty);
        }
        if value.contains(':') && !value.starts_with('/') {
            Ok(Self::Tcp(value))
        } else {
            Ok(Self::Unix(PathBuf::from(value)))
        }
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unix(path) => path.display().fmt(formatter),
            Self::Tcp(address) => address.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ServerDiscoveryError {
    #[error("Neovim server address is empty")]
    Empty,
    #[error("no Neovim server found; set $NVIM or pass --server")]
    Missing,
}

/// # Errors
/// Returns an error when neither the override nor `$NVIM` identifies a server.
pub fn discover_server(
    override_value: Option<&str>,
) -> Result<ServerAddress, ServerDiscoveryError> {
    discover_server_with(override_value, |name| env::var(name).ok())
}

/// # Errors
/// Returns an error when neither source identifies a non-empty server.
pub fn discover_server_with(
    override_value: Option<&str>,
    env_value: impl FnOnce(&str) -> Option<String>,
) -> Result<ServerAddress, ServerDiscoveryError> {
    if let Some(server) = override_value {
        return ServerAddress::parse(server);
    }
    let server = env_value("NVIM").ok_or(ServerDiscoveryError::Missing)?;
    ServerAddress::parse(server)
}

#[cfg(test)]
mod tests {
    use super::{ServerAddress, ServerDiscoveryError, discover_server_with};
    use std::path::PathBuf;

    #[test]
    fn explicit_server_overrides_nvim_environment() {
        let result = discover_server_with(Some("/tmp/explicit.sock"), |_| {
            Some("/tmp/environment.sock".into())
        });
        assert_eq!(
            Ok(ServerAddress::Unix(PathBuf::from("/tmp/explicit.sock"))),
            result
        );
    }

    #[test]
    fn nvim_environment_is_default() {
        let result = discover_server_with(None, |_| Some("127.0.0.1:7777".into()));
        assert_eq!(Ok(ServerAddress::Tcp("127.0.0.1:7777".into())), result);
    }

    #[test]
    fn missing_server_has_actionable_error() {
        let result = discover_server_with(None, |_| None);
        assert_eq!(Err(ServerDiscoveryError::Missing), result);
    }

    #[test]
    fn empty_override_does_not_fall_back() {
        let result = discover_server_with(Some(""), |_| Some("/tmp/nvim.sock".into()));
        assert_eq!(Err(ServerDiscoveryError::Empty), result);
    }
}
