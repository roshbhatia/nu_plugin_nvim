use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::SystemTime;

use thiserror::Error;

use crate::RpcClient;

const DISCOVERY_TIMEOUT: Duration = Duration::from_millis(200);

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
    #[error("found {0} Neovim servers; run `nuvim` to choose one or pass --server")]
    Multiple(usize),
}

/// # Errors
/// Returns an error when neither the override nor `$NVIM` identifies a server.
pub fn discover_server(
    override_value: Option<&str>,
) -> Result<ServerAddress, ServerDiscoveryError> {
    match discover_server_with(override_value, |name| env::var(name).ok()) {
        Err(ServerDiscoveryError::Missing) => match discover_servers().as_slice() {
            [] => Err(ServerDiscoveryError::Missing),
            [server] => Ok(server.clone()),
            servers => Err(ServerDiscoveryError::Multiple(servers.len())),
        },
        result => result,
    }
}

/// Returns live Neovim servers in newest-first order.
#[must_use]
pub fn discover_servers() -> Vec<ServerAddress> {
    let user = env::var("USER").unwrap_or_default();
    let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let temporary = env::var_os("TMPDIR").map_or_else(env::temp_dir, PathBuf::from);
    let candidates = discover_server_candidates(runtime.as_deref(), &temporary, &user);
    live_servers_with(candidates, is_neovim_server)
}

fn discover_server_candidates(
    runtime: Option<&Path>,
    temporary: &Path,
    user: &str,
) -> Vec<ServerAddress> {
    let mut roots = Vec::new();
    if let Some(runtime) = runtime {
        roots.push(runtime.to_path_buf());
    }
    if !user.is_empty() {
        roots.push(temporary.join(format!("nvim.{user}")));
    }

    let mut found = roots
        .iter()
        .flat_map(|root| socket_candidates(root, 2))
        .collect::<Vec<_>>();
    found.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    found.dedup_by(|left, right| left.0 == right.0);
    found
        .into_iter()
        .map(|(path, _)| ServerAddress::Unix(path))
        .collect()
}

fn live_servers_with(
    candidates: Vec<ServerAddress>,
    mut probe: impl FnMut(&ServerAddress) -> bool,
) -> Vec<ServerAddress> {
    candidates
        .into_iter()
        .filter(|server| probe(server))
        .collect()
}

fn is_neovim_server(address: &ServerAddress) -> bool {
    let Ok(mut client) = RpcClient::connect_with_timeout(address, DISCOVERY_TIMEOUT) else {
        return false;
    };
    client.call("nvim_get_api_info", vec![]).is_ok()
}

fn socket_candidates(root: &Path, depth: usize) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() && depth > 0 {
            found.extend(socket_candidates(&path, depth - 1));
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("nvim.") && name.ends_with(".0") {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((path, modified));
        }
    }
    found
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
    use super::{
        ServerAddress, ServerDiscoveryError, discover_server_candidates, discover_server_with,
        live_servers_with,
    };
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use tempfile::tempdir;

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

    #[test]
    fn standard_runtime_sockets_are_discovered() {
        let directory = tempdir().expect("temporary directory should exist");
        let session = directory.path().join("nvim.user").join("session");
        std::fs::create_dir_all(&session).expect("session directory should exist");
        let socket = session.join("nvim.42.0");
        let _listener = UnixListener::bind(&socket).expect("socket should bind");

        let servers = discover_server_candidates(Some(directory.path()), directory.path(), "user");

        assert_eq!(servers, vec![ServerAddress::Unix(socket)]);
    }

    #[test]
    fn stale_runtime_sockets_are_removed_before_selection() {
        let candidates = vec![
            ServerAddress::Unix(PathBuf::from("/tmp/nvim.stale.0")),
            ServerAddress::Unix(PathBuf::from("/tmp/nvim.live.0")),
        ];

        let servers = live_servers_with(candidates, |address| address.to_string().contains("live"));

        assert_eq!(
            servers,
            vec![ServerAddress::Unix(PathBuf::from("/tmp/nvim.live.0"))]
        );
    }
}
