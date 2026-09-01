use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nuvim_protocol::{HandleKind, NvimHandle, RpcClient, ServerAddress};
use rmpv::Value;
use tempfile::TempDir;

struct HeadlessNvim {
    child: Child,
    _directory: TempDir,
    address: ServerAddress,
}

impl HeadlessNvim {
    fn start() -> Option<Self> {
        if Command::new("nvim")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("skipping headless integration test because nvim is unavailable");
            return None;
        }
        let directory = tempfile::tempdir().expect("temporary directory should exist");
        let socket = directory.path().join("nvim.sock");
        let mut child = Command::new("nvim")
            .args(["--headless", "--clean", "-n", "-u", "NONE", "--listen"])
            .arg(&socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("headless Neovim should start");
        wait_for_socket(&socket, &mut child);
        Some(Self {
            child,
            _directory: directory,
            address: ServerAddress::Unix(socket),
        })
    }
}

impl Drop for HeadlessNvim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn creates_modifies_and_reads_a_headless_buffer() {
    let Some(server) = HeadlessNvim::start() else {
        return;
    };
    let mut client = RpcClient::connect(&server.address).expect("client should connect");
    let raw_buffer = client
        .call(
            "nvim_create_buf",
            vec![Value::Boolean(true), Value::Boolean(false)],
        )
        .expect("buffer should be created");
    let buffer = NvimHandle::from_rpc_value(&raw_buffer).expect("buffer should be a handle");
    assert_eq!(HandleKind::Buffer, buffer.kind);
    client
        .call(
            "nvim_buf_set_lines",
            vec![
                raw_buffer.clone(),
                Value::from(0),
                Value::from(-1),
                Value::Boolean(true),
                Value::Array(vec![Value::from("alpha"), Value::from("beta")]),
            ],
        )
        .expect("buffer lines should change");
    let lines = client
        .call(
            "nvim_buf_get_lines",
            vec![
                raw_buffer,
                Value::from(0),
                Value::from(-1),
                Value::Boolean(true),
            ],
        )
        .expect("buffer lines should be read");
    assert_eq!(
        Value::Array(vec![Value::from("alpha"), Value::from("beta")]),
        lines
    );
}

fn wait_for_socket(path: &Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        if let Ok(Some(status)) = child.try_wait() {
            panic!("headless Neovim exited before listening: {status}");
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("headless Neovim did not create {}", path.display());
}
