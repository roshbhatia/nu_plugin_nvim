# Editor automation

These commands use the running editor's buffers, language servers, and parsers.
Select a server once and pass its address on subsequent calls.
Rows are zero-based, columns are UTF-8 byte offsets, and range ends are exclusive.

## Guarded transforms

Preview a transformation:

```nu
let server = (nuvim | get server)
nuvim transform { str uppercase } --selection --preview --server $server
```

Apply a transformation to a buffer:

```nu
nuvim transform { |text| $text | from json | to json } --buffer 7 --server $server
```

Replace an exact range:

```nu
nuvim transform { str uppercase } --buffer 7 --server $server --start-row 0 --start-column 3 --end-row 0 --end-column 5
```

Specify all four position flags for a range.
`--selection` cannot combine with a buffer or range argument.
The closure receives captured text as pipeline input and its first argument.
It must return a string or a list of strings.

Nuvim captures the buffer, range, and `changedtick` together before calling the closure.
It checks `changedtick` and applies the edit within one Lua call, without yielding between those operations.
A buffer switch does not redirect the edit.
An intervening edit, unload, closure failure, or interrupted command prevents Nuvim from applying the result.
A successful transform creates one undoable edit and does not save the file.

The result includes `server`, `target`, `before`, `after`, `applied`, and the resulting `changedtick`.
Preview returns the proposed text without applying it; its resulting `changedtick` is null.
Preview still executes the closure.
Use closures without external side effects, because Nuvim cannot undo actions performed by a closure.

## Quickfix identities and history

Create a separate list and retain its identity:

```nu
let review = ([{path: "README.md", row: 2, column: 0, text: "Review introduction"}]
  | nuvim quickfix set --action new --title Review --context {owner: review} --server $server)
nuvim quickfix history --server $server
nuvim quickfix get --id $review.id --details --server $server
```

`--action` accepts `new`, `append`, or `replace`; its default remains `replace`.
`new` creates a history entry and rejects `--id`.
`append` preserves an existing title unless `--title` overrides it.
An update with `--id` changes that list without selecting another history entry.
An unknown ID fails before mutation.

List metadata includes `id`, `nr`, `title`, `count`, `idx`, `changedtick`, `context`, `current`, `server`, and `window`.
`get` returns items by default; `--details` adds metadata around `items`.
Use `--window` on `get`, `set`, or `history` for window-local location lists.
Window `0` means the current window.
Use `nuvim command lopen` to open the current window's location list.
List IDs belong to one editor and list type; retain the server and window alongside them.

## Structured code queries

```nu
nuvim symbols --server $server | select name kind path row column
nuvim references --row 10 --column 4 --server $server
nuvim definition --row 10 --column 4 --server $server
nuvim node --row 10 --column 4 --server $server
```

Symbols, references, and definitions query attached LSP clients supporting the requested operation.
Results include the client ID and name; duplicate locations from different clients remain separate records.
Symbol records include `name`, `kind`, and optional `container` fields.
LSP positions convert from each client's encoding into byte offsets.
Queries fail when clients report errors, exceed the timeout, or the source buffer changes during the request.
An unsupported operation produces an explicit error; a supported query with no matches returns an empty list.

`--timeout` accepts 1 through 8000 milliseconds and defaults to 2000 for LSP queries.
`node` requires an installed Tree-sitter parser and parses the buffer before querying it.
It returns the smallest named node followed by its ancestors, including their types, text, and ranges.

Queries use the cursor position for the current buffer.
For a non-current buffer, the default position is row 0, column 0.
Provide both position flags when overriding that position.

## Event streams

```nu
nuvim watch buffer --server $server --initial
nuvim watch save --server $server
nuvim watch diagnostics --server $server
```

Each watch captures one buffer and starts with a `ready` record containing its server and buffer ID.
Buffer watches preserve Neovim's lines, changedtick, and detach event names.
Line events include `changedtick`, `firstline`, `lastline`, `lines`, and `more`.
`lastline` is exclusive; `-1` identifies an initial snapshot.
Preserve multipart ordering while `more` is true.
A null `changedtick` describes a display preview, not a stored text change.

Save events use `BufWritePost`; diagnostic events use `DiagnosticChanged` and include diagnostic records.
These events include the buffer path and `changedtick`.
Closing the pipeline or interrupting Nushell closes the socket and stops the reader.
Neovim detaches buffer listeners when the channel closes.
An editor timer removes save and diagnostic autocmds after detecting a closed channel, at 250-millisecond intervals.

The reader queues at most 64 events.
If the consumer falls behind, the watch closes its connection and emits an overflow error after queued events.
Restart the watch and request a new snapshot after overflow or disconnection.
Watches do not reconnect or replay missed events automatically.

Filter readiness and changedtick events when consuming text changes:

```nu
nuvim watch buffer --server $server
| where event == nvim_buf_lines_event
| each { |event| $event.lines }
```

Keep event handlers from writing to their own watched buffer unless they explicitly prevent feedback loops.
