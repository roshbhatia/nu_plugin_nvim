# Build the Nuvim structured-data bridge

> Status: draft · Authors: Roshan Bhatia · Created: 2026-09-01

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119).

Scope includes:

- A `nu-plugin` binary that exposes `nuvim` pipeline commands.
- A MessagePack-RPC client for an existing Neovim server.
- An in-process `nvim-oxi` module that invokes Nushell.
- Explicit conversions between MessagePack, Nushell, and Neovim Lua values.
- A protocol path for future live handlers and event streams.

Scope does not include:

- A complete translation of `vim.api` into Nushell commands.
- Embedded Nushell execution inside Neovim.
- Live handler registration or event streaming in version 0.1.
- A long-lived connection pool or automatic reconnection.

## Summary

Nuvim maps selected Neovim objects into native Nushell records, lists, and streams.
The Nushell plugin connects to `$NVIM` or `--server` through MessagePack-RPC.
The Neovim module uses `nvim-oxi` in process and runs bounded Nushell child processes.

## Motivation

Neovim stores editor state as buffers, windows, tabs, diagnostics, and events.
Nushell transforms structured values, but the `nvim` executable exposes command-oriented remote control.
The bridge needs pipeline sources and sinks without hiding Neovim state inside formatted text.

### Goals

- `nuvim buffers | where modified` MUST operate on Nushell records.
- Commands MUST discover `$NVIM` and accept `--server` as an override.
- Rows and columns MUST be zero-based at the public Nuvim boundary.
- RPC, conversion, and connection failures MUST return contextual errors without panics.
- Nix MUST define developer tooling and build outputs.

### Non-Goals

- Version 0.1 MUST NOT register every function reported by `nvim_get_api_info()`.
- Version 0.1 MUST NOT implement `nuvim expose` or `nuvim watch`.
- Version 0.1 SHOULD NOT add Nushell `CustomValue` handles until commands accept ordinary records reliably.

## Proposal

The workspace contains three Rust crates.

- `nuvim-protocol` owns MessagePack values, Neovim handles, RPC framing, server discovery, API metadata, and quickfix records.
- `nu-plugin-nuvim` owns Nushell signatures, pipeline handling, labeled errors, and Nushell value conversion.
- `nvim-nu` owns the `nvim-oxi` module returned by `require("nu")`.

The repository also contains `plugin/nu.lua`, examples, tests, a flake, and this design note.

### User workflows

Each Nushell command opens one connection, performs bounded calls, and closes the connection.
This lifecycle avoids hidden stale sockets and keeps version 0.1 understandable.
The client MUST accept Unix sockets and TCP addresses supported by Neovim `--listen`.

The initial command surface is:

```text
nuvim context
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

Each subcommand accepts `--server` because Nushell does not inherit parent command flags.
The name `nuvim` prevents collision with the existing `nvim` executable.

### Notes, constraints, caveats

- Neovim RPC uses request, response, and notification arrays over a MessagePack byte stream.
- The client sends one request at a time, so Neovim's reverse-response ordering cannot reorder local requests.
- The response loop MUST skip and preserve notifications until it receives the matching response.
- Neovim encodes buffer, window, and tab handles as MessagePack extension values.
- Raw handle results become tagged records with `kind`, `id`, and `server` fields.
- Structured command records MAY expose an `id` inside the surrounding buffer, window, or tab record.
- `nvim_get_api_info()` supplies method names and parameter metadata for validation.
- Version 0.1 uses metadata to reject unknown raw calls and exposes a cache boundary for later completions.

### Risks and mitigations

- A blocking call can pause Nushell. RPC reads and writes MUST use timeouts with the server in the error.
- A Nushell child can pause Neovim. The companion module MUST run it outside Neovim's main thread.
- Quickfix uses one-based positions. Conversion MUST add or subtract one only at the Neovim boundary.
- Neovim maps can contain non-string keys. Nushell conversion MUST return a tagged map instead of dropping keys.
- MessagePack integers can exceed `i64`. Conversion MUST return a tagged integer or an error.

## Design Details

### RPC transport

`RpcClient` owns a buffered reader, writer, next request identifier, server identity, and queued notifications.
Requests use `[0, id, method, args]`.
Responses use `[1, id, error, result]`.
Notifications use `[2, method, args]`.

Malformed array lengths, message types, identifiers, or response errors MUST produce typed errors.
The decoder MUST keep stream position valid after a notification.

The implementation follows Neovim's current [API and RPC contract](https://neovim.io/doc/user/api/).
The contract permits API clients to call functions, listen for events, and receive remote calls.

### Values and handles

MessagePack nil, booleans, signed integers, floats, strings, binary values, arrays, and string-keyed maps map directly to Nushell values.
Unsigned integers map directly only when `i64` can represent them.
Neovim extensions with metadata type IDs map to tagged handles.
Unknown extensions map to `{type: "msgpack-ext", tag: <int>, data: <binary>}`.
Maps with non-string keys map to `{type: "msgpack-map", entries: [{key, value}]}`.

The reverse conversion recognizes both tagged forms.
It MUST reject unsupported Nushell values with the source span and value type.

`CustomValue` can carry server identity, handle type, and handle ID across pipelines.
Version 0.1 defers it because plugin custom values add serialization and lifecycle hooks.
The protocol crate keeps `NvimHandle` independent from Nushell so a later wrapper does not change RPC code.

### Structured commands

`nuvim context` combines current buffer, window, tab, cursor, mode, and working directory calls.
`nuvim buffers` returns one record per listed buffer.
`nuvim text` returns buffer identity, the selected range, `lines`, and joined `text`.
`nuvim selection` returns the last visual selection with zero-based endpoints and text.
`nuvim open` consumes path strings and returns the opened buffer records.
`nuvim replace` consumes text or lines and replaces the selection or whole buffer.
`nuvim diagnostics` returns normalized diagnostic records.
Quickfix commands use the documented zero-based schema and convert at the boundary.
`nuvim scratch` renders strings as text, string lists as lines, and other values as NUON.

`nuvim call` accepts a method plus a list of arguments.
It loads `nvim_get_api_info()` before the raw call and validates the method name and argument count.
The metadata cache boundary MAY provide dynamic completion after Nushell stabilizes that plugin API.
`nuvim lua` calls `nvim_exec_lua` with source and optional arguments.

### Neovim companion

The `nvim-nu` crate builds a `cdylib` that returns `eval`, `filter`, and `call` functions.
The Lua loader resolves the Nix-built module and returns `require("nvim_nu")` as `require("nu")`.
Each function serializes input as JSON, starts `nu --no-config-file`, and parses JSON output.
Version 0.1 calls are synchronous, so large pipelines can block Neovim until Nushell exits.
A later asynchronous API SHOULD perform process I/O on worker threads and schedule callbacks on Neovim's main thread.

### Future live handlers

`nuvim expose <name> <closure>` should keep the plugin process alive and disable plugin garbage collection.
`EngineInterface::eval_closure_with_stream()` is suitable because it accepts `PipelineData` and returns streaming output.
The plugin should register its callback channel through a session record stored in Neovim.

The session protocol should use versioned envelopes:

```text
request:  {version: 1, id, handler, input, context}
response: {version: 1, id, result} | {version: 1, id, error}
```

The callback transport MUST remain separate from the command RPC client.
This split prevents a Neovim request from waiting on the same channel needed to return its result.

### Future event streams

`nuvim watch buffer` should call `nvim_buf_attach` and keep reading RPC notifications.
Remote `nvim_buf_lines_event` notifications include changed lines, zero-based ranges, and `changedtick`.
The command should map each notification into a Nushell list stream without collecting it.

Autocommand watches should create a Neovim autocmd that calls `rpcnotify()` on the client channel.
Cleanup MUST delete temporary autocmds and detach buffers when Nushell interrupts the stream.
The existing RPC decoder and `PipelineData` command boundary support this without a second wire format.

### Validation

- MessagePack conversion is correct when unit tests round-trip every direct and tagged value.
- Quickfix positions are correct when unit tests prove zero-to-one-based conversion in both directions.
- Server discovery is correct when tests cover override, `$NVIM`, missing values, Unix sockets, and TCP addresses.
- Transport is correct when a headless Neovim test creates, changes, and reads a buffer through `RpcClient`.
- Command behavior is correct when Nushell examples run against a headless server.
- The companion module is loadable when headless Neovim requires `nu` from the Nix package.
- Nix integration is complete when `nix flake check` builds both artifacts and runs the test suite.

## Drawbacks

Each command creates a connection and repeats API metadata calls for raw methods.
The process cost is acceptable for version 0.1 but will affect repeated small calls.
Lua snippets implement some aggregate operations because the remote API has no single equivalent call.
The synchronous companion API can block Neovim while a Nushell process runs.

## Alternatives

- Run `nvim --server ... --remote-expr` for every command. This loses typed MessagePack values and notifications.
- Use `nvim-oxi` in the Nushell plugin. It requires Neovim process symbols and cannot attach to an existing server.
- Generate one Nushell command per Neovim API function. This copies an object API into a pipeline shell and creates unstable surface area.
- Embed the Nushell engine in Neovim now. This increases binary size and engine-state complexity before the basic data model is proven.
- Add `CustomValue` handles now. Tagged records keep version 0.1 inspectable and preserve a migration path.

## Dependencies and resources needed

- Rust 1.95 or newer, as required by `nu-plugin 0.115.1`.
- Nushell 0.115.1 for plugin registration and runtime tests.
- Neovim 0.10 or newer, with development validation on 0.12.5.
- Nix packages for Rust, Nushell, Neovim, formatting, linting, and checks.

## Implementation history

- 2026-09-01 created after reviewing `nu-plugin 0.115.1`, Neovim 0.12.5 API metadata, and `nvim-oxi 0.6.0`.
