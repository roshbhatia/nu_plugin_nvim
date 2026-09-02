# Build the Nuvim structured-data bridge

> Status: draft · Authors: Roshan Bhatia · Created: 2026-09-01

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

Scope includes:

- A Nushell plugin that exposes `nuvim` pipeline commands.
- A MessagePack-RPC client for existing Neovim servers.
- A generated Rust client derived from Neovim API metadata.
- Explicit conversions between MessagePack and Nushell values.
- A protocol path for future event streams.

Scope does not include:

- Running Nushell inside Neovim.
- An editor-side Neovim plugin or native module.
- Live handler registration or event streaming in version 0.1.
- A long-lived connection pool or automatic reconnection.

## Summary

Nuvim maps Neovim state into native Nushell records, lists, and streams. The
plugin attaches to an existing Neovim server through MessagePack-RPC. A build
tool reads `nvim --api-info` and generates the Rust methods and metadata used by
the plugin, so normal commands do not maintain method names or arity by hand.

## Motivation

Neovim stores editor state as buffers, windows, tabs, diagnostics, and events.
Nushell transforms structured values, but Neovim's command-line remote control
returns formatted text. Nuvim needs typed pipeline sources and sinks without a
second runtime inside the editor.

### Goals

- `nuvim buffers | where modified` MUST operate on Nushell records.
- Commands MUST accept `--server`, prefer `$NVIM`, and discover standard runtime sockets when both are absent.
- Rows and columns MUST be zero-based at the public Nuvim boundary.
- RPC, conversion, and connection failures MUST return contextual errors without panics.
- Generated Rust methods MUST match the Neovim API used by the Nix build.
- Nix MUST define developer tooling, checks, and package outputs.

### Non-Goals

- Version 0.1 MUST NOT register every API function as a Nushell command.
- Version 0.1 MUST NOT implement `nuvim expose` or `nuvim watch`.
- Version 0.1 SHOULD NOT add Nushell `CustomValue` handles until ordinary records prove the command model.

## Proposal

The workspace contains three Rust crates.

- `nuvim-protocol` owns RPC framing, generated methods, handles, server discovery, and quickfix records.
- `nu-plugin-nuvim` owns Nushell signatures, pipelines, labeled errors, and value conversion.
- `nuvim-codegen` converts `nvim --api-info` into deterministic Rust source.

The generator writes `crates/nuvim-protocol/src/generated.rs`. The generated
file contains API version constants, function metadata, and one `RpcClient`
method per reported Neovim function. The repository tracks this file so source
builds do not need code generation before compilation.

### User workflows

Each Nushell command opens one connection, performs bounded calls, and closes
the connection. The client MUST accept Unix sockets and TCP addresses supported
by Neovim `--listen`.

The initial command surface is:

```text
nuvim context
nuvim servers
nuvim buffers
nuvim text
nuvim selection
nuvim open
nuvim replace
nuvim diagnostics
nuvim quickfix get
nuvim quickfix set
nuvim quickfix open
nuvim scratch
nuvim call
nuvim lua
```

Each subcommand accepts `--server` because Nushell does not inherit parent
command flags. The name `nuvim` prevents collision with the `nvim` executable.

Nuvim scans the runtime paths used by `serverlist()` and `serverstart()`. It
filters stale sockets with a bounded RPC connection and orders live sessions by
socket modification time. One live session is selected automatically. Multiple
sessions require `--server` or an explicit shell selection.

### Notes, constraints, caveats

- Neovim RPC uses request, response, and notification arrays over a MessagePack byte stream.
- The client sends one request at a time, so responses cannot reorder local requests.
- The response loop MUST preserve notifications until the matching response arrives.
- Buffer, window, and tab handles use MessagePack extension values.
- Raw handle results become tagged records with `kind`, `id`, and `server` fields.
- `nuvim call` MUST validate method names and arity against generated metadata.
- The generated source MUST remain deterministic for the same API specification.

### Risks and mitigations

- A blocking call can pause Nushell. RPC reads and writes use bounded timeouts and include the server in each error.
- Neovim API drift can leave the checked-in client stale. The Nix check regenerates the file in check mode and fails on a difference.
- Quickfix uses one-based positions. Conversion adds or subtracts one only at the Neovim boundary.
- MessagePack maps can contain non-string keys. Conversion returns a tagged map instead of dropping keys.
- MessagePack integers can exceed `i64`. Conversion returns a tagged integer or an error.

## Design Details

### RPC transport

`RpcClient` owns a buffered stream, the next request identifier, server identity,
and queued notifications. Requests use `[0, id, method, args]`. Responses use
`[1, id, error, result]`. Notifications use `[2, method, args]`.

Malformed array lengths, message types, identifiers, and remote errors MUST
produce typed errors. The decoder MUST keep the stream valid after a
notification. The implementation follows Neovim's current
[API and RPC contract](https://neovim.io/doc/user/api/).

### Generated API client

`nuvim-codegen` runs `nvim --api-info`, accepts the top-level metadata map, and
sorts functions by name. It normalizes API parameter names into Rust identifiers
and rejects duplicate generated identifiers. Every generated method accepts
`rmpv::Value` parameters and returns `Result<Value, RpcError>` through the shared
transport.

Structured Nushell commands call these generated methods. The dynamic `nuvim
call` command still invokes `RpcClient::call`, but it first looks up the method
in the generated metadata and checks its argument count.

### Values and handles

MessagePack nil, booleans, signed integers, floats, strings, binary values,
arrays, and string-keyed maps map directly to Nushell values. Unsigned integers
map directly only when `i64` can represent them. Neovim extensions with known
type IDs map to tagged handles. Unknown extensions map to `{type:
"msgpack-ext", tag: <int>, data: <binary>}`. Maps with non-string keys map to
`{type: "msgpack-map", entries: [{key, value}]}`.

The reverse value conversion recognizes both tagged forms because Nushell sends
arguments into Neovim RPC. It MUST reject unsupported Nushell values with the
source span and value type.

### Structured commands

`nuvim context` combines current buffer, window, tab, cursor, mode, and working
directory calls. `nuvim buffers` returns one record per listed buffer. `nuvim
text` returns buffer identity, the selected range, `lines`, and joined `text`.
The remaining commands expose selection, file opening, replacement, diagnostics,
quickfix, scratch buffers, raw calls, and Lua evaluation through the same RPC
client.

### Future event streams

`nuvim watch buffer` SHOULD call `nvim_buf_attach` and keep reading RPC
notifications. It should map `nvim_buf_lines_event` notifications into a
Nushell list stream without collecting them. Signal cleanup MUST detach buffers
and delete temporary autocmds.

### Validation

- Generated API parity is checked by `cargo run -p nuvim-codegen -- --check`; inverse: a changed Neovim specification makes the check fail.
- MessagePack conversion is checked by unit tests that round-trip direct and tagged values; inverse: unsupported values return labeled errors.
- Quickfix positions are checked by zero-to-one-based conversion tests in both directions.
- Server discovery is checked with overrides, `$NVIM`, missing values, Unix sockets, and TCP addresses.
- Transport is checked against headless Neovim by creating, changing, and reading a buffer.
- Command behavior is checked by running Nushell examples against a headless server.
- Reverse bridge removal is checked by the absence of `nvim-oxi`, `nvim-nu`, and `require("nu")` from package outputs and source references.
- Nix integration is checked by `nix flake check`, which builds the package and runs the workspace tests.

## Drawbacks

Each command creates a connection. This adds setup cost to repeated small calls.
The checked-in generated client is large. Lua snippets still implement aggregate
operations that have no single remote API call.

## Alternatives

- Use `nvim --server ... --remote-expr` for every command. This loses typed MessagePack values and notifications.
- Use `nvim-oxi` in the Nushell plugin. It requires Neovim process symbols and cannot attach to an existing server.
- Use `nvim-rs`. It adds an async runtime and LGPL-3.0 code to a synchronous MIT workspace. Event streaming MAY justify revisiting this choice.
- Keep an editor-side reverse bridge. This adds a second control direction and process lifecycle without helping Nushell inspect or update Neovim state.
- Register every generated function as a Nushell command. This copies an object API into the shell and creates an unstable public surface.
- Add `CustomValue` handles now. Tagged records keep version 0.1 inspectable and preserve a migration path.

## Dependencies and resources needed

- Rust 1.95 or newer, as required by `nu-plugin 0.115.1`.
- Nushell 0.115.1 for plugin registration and runtime tests.
- Neovim 0.10 or newer, with generation and validation on 0.12.5.
- Nix packages for Rust, Nushell, Neovim, formatting, linting, and checks.

## Implementation history

- 2026-09-01 created after reviewing `nu-plugin 0.115.1`, Neovim 0.12.5 API metadata, and `nvim-oxi 0.6.0`.
- 2026-09-02 removed the reverse bridge and generated the Rust RPC client from Neovim API metadata.
