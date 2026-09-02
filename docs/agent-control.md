# Agent control contract

Nuvim lets an agent inspect and control an existing Neovim process through
Nushell data. It uses direct MessagePack-RPC calls. It does not simulate keys or
require an editor-side plugin.

## Session selection

List live editors and select one server address:

```nu
let editor = (nuvim servers | first)
let server = $editor.server
```

Pass `--server $server` on every later command. This keeps an automation bound
to one editor when another Neovim process starts.

## Observe

Read context before changing state:

```nu
let context = (nuvim context --server $server)
let buffers = (nuvim buffers --server $server)
let text = (nuvim text --server $server)
let diagnostics = (nuvim diagnostics --server $server)
```

Rows and columns are zero-based. Columns are UTF-8 byte offsets.

## Navigate

Open a path, select a loaded buffer, or move the cursor:

```nu
let opened = (nuvim open --server $server README.md | first)
nuvim buffer use $opened.id --server $server
nuvim cursor set 10 2 --server $server
```

`nuvim open` makes its first path current. It loads later paths without changing
the current buffer.

## Edit

Insert at a position by omitting the end position:

```nu
"prefix: " | nuvim edit 10 2 --server $server
```

Replace an exact range by supplying its exclusive end position:

```nu
"replacement" \
| nuvim edit 10 2 --end-row 10 --end-column 8 --server $server
```

Use `--buffer <id>` to edit a non-current buffer. Use `nuvim replace` only when
the whole buffer or last visual selection is the intended target.

Edits remain unsaved. Save only when the workflow requires it:

```nu
nuvim command "write" --server $server
```

## Verify

Read the affected range after each mutation:

```nu
nuvim text --start 10 --end 11 --server $server
```

Use `nuvim call` or `nuvim lua` only when the stable commands cannot express an
operation. Prefer direct editor state changes over `nvim_input` and simulated
keys.
