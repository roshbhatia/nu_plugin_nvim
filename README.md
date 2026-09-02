# Nuvim

![Nuvim structured buffer data](docs/nuvim.png)

Nuvim makes Neovim state available as native Nushell data.
It also provides a small Rust Neovim module that sends values through Nushell pipelines.

Rows and columns are zero-based in every Nuvim command.
Columns are UTF-8 byte offsets, which matches the Neovim API.

## Architecture

The Nushell plugin connects to an existing Neovim server through MessagePack-RPC.
It discovers the parent editor through `$NVIM`, or uses a command-specific `--server` override.
This side cannot use `nvim-oxi` because that crate needs Neovim symbols inside the editor process.

The `nvim-nu` crate uses `nvim-oxi` inside Neovim.
Its `require("nu")` API starts bounded Nushell child processes for reverse-direction evaluation.

The shared `nuvim-protocol` crate owns RPC framing, handle types, server discovery, API metadata, and quickfix conversion.
See [the architecture note](docs/architecture.md) for the protocol and future session design.

## Nix installation

Build every artifact through the flake:

```sh
nix build
```

Register the Nushell plugin:

```nu
plugin add (realpath ./result/bin/nu_plugin_nuvim)
```

Nushell loads the registered `nuvim` commands on its next start.
Use `plugin use nuvim` to reload them in an existing session.

Add the Neovim package output to `runtimepath`, then require the module:

```lua
vim.opt.runtimepath:prepend("/path/to/result/share/nvim/site")
local nu = require("nu")
```

Nix consumers can use `packages.<system>.default`, `packages.<system>.nu-plugin`, or `packages.<system>.nvim-plugin`.
The default package contains both sides.
The Nix-built Neovim module uses the Nushell binary from its package closure.
Source builds use `nu` from `PATH`, or `NUVIM_NU_BIN` when set.

Use the declared development shell for every Rust and Neovim dependency:

```sh
nix develop
cargo test --workspace
./hack/screenshots.sh
```

## Nushell commands

The version 0.1 surface is:

```text
nuvim context
nuvim buffers
nuvim text [--buffer <id>] [--start <row>] [--end <row>]
nuvim selection
nuvim open [paths...]
nuvim replace [--selection] [--buffer <id>]
nuvim diagnostics
nuvim quickfix get
nuvim quickfix set [--title <text>]
nuvim quickfix open [--height <rows>]
nuvim scratch [--name <name>] [--filetype <name>]
nuvim call <method> [arguments...]
nuvim lua <code> [arguments...]
```

Every command accepts `--server <socket-or-host:port>`.
Without that flag, Nuvim uses `$NVIM`.

Buffer, window, and tab records include IDs and server identity.
Raw handle results use `{type: "nvim-handle", kind, id, server}` records instead of bare integers.
Unknown MessagePack extensions and maps with non-string keys use tagged records without data loss.

`nuvim call` reads `nvim_get_api_info()` before each call.
It rejects unknown methods and incorrect argument counts.
The metadata layer can provide dynamic completions later without changing the transport.

## Demo

Inspect the current editor state:

```nu
nuvim context
```

Find modified buffers:

```nu
nuvim buffers
| where modified
| select id path filetype
```

Filter diagnostics:

```nu
nuvim diagnostics
| where severity == "ERROR"
| select path row column message
```

Open paths produced by another command:

```nu
git diff --name-only
| lines
| where { not ($in | is-empty) }
| nuvim open
```

Replace the last visual selection:

```nu
nuvim selection
| get text
| str uppercase
| nuvim replace --selection
```

Send ripgrep matches to quickfix:

```nu
rg TODO --json
| lines
| each { $in | from json }
| where type == "match"
| each { |event|
    {
      path: $event.data.path.text
      row: ($event.data.line_number - 1)
      column: $event.data.submatches.0.start
      text: ($event.data.lines.text | str trim --right)
      type: "I"
    }
  }
| nuvim quickfix set --title "TODO"
| nuvim quickfix open
```

Raw escape hatches preserve structured values:

```nu
nuvim call nvim_get_current_buf
nuvim lua 'return vim.bo.filetype'
```

## Quickfix schema

Quickfix input accepts partial records.
Missing positions stay unset.

```nu
{
  path: "/project/src/main.rs"
  row: 41
  column: 6
  end_row: null
  end_column: null
  text: "some diagnostic"
  type: "E"
}
```

Nuvim adds one only when it sends positions into Neovim quickfix functions.
`nuvim quickfix get` subtracts one before returning records.

## Scratch buffers

Strings become text, and string lists become lines.
Other Nushell values use Nushell's expanded structured representation.

```nu
ls | nuvim scratch --name files --filetype nuon
```

## Neovim API

The companion module exposes:

```lua
local nu = require("nu")

nu.eval("ls | where size > 10mb")
nu.filter("str uppercase", "hello")
nu.call("str length", "hello")
```

Version 0.1 uses JSON between Neovim and each child process.
These calls are synchronous and can block Neovim during long pipelines.
The errors include the operation, exit status, and complete Nushell stderr.

## Recipes

The [recipes directory](recipes) contains one folder per workflow.
Each folder has a runnable Nushell script and a focused README.

## Deferred work

`CustomValue` handles remain deferred until ordinary records prove the command model.
The protocol already keeps handle type, ID, and server identity separate from Nushell values.

`nuvim expose` should keep a plugin command alive and call `EngineInterface::eval_closure_with_stream()` for registered closures.
It needs a second callback channel so Neovim never waits on the command channel that must return its result.
Requests and responses should use versioned IDs, structured results, and explicit errors.

`nuvim watch buffer` should call `nvim_buf_attach` and map `nvim_buf_lines_event` notifications into a Nushell list stream.
Autocommand watches should call `rpcnotify()` on the client channel.
Signal cleanup must detach buffers and delete temporary autocmds.

## Current API sources

- [`nu-plugin 0.115.1`](https://docs.rs/nu-plugin/0.115.1/nu_plugin/)
- [Neovim API and MessagePack-RPC](https://neovim.io/doc/user/api/)
- [`nvim-oxi 0.6.0`](https://docs.rs/nvim-oxi/0.6.0/nvim_oxi/)
