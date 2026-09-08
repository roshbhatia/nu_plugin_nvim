# Nuvim recipes

Each folder contains one runnable Nushell workflow and its assumptions.

- [`agent-control`](agent-control) drives cursor, buffer, and range-edit actions.
- [`modified-buffers`](modified-buffers) filters buffer records.
- [`error-diagnostics`](error-diagnostics) filters diagnostic records.
- [`git-diff-open`](git-diff-open) opens changed paths.
- [`selection-uppercase`](selection-uppercase) transforms selected text.
- [`rg-quickfix`](rg-quickfix) converts ripgrep JSON into quickfix records.
- [`scratch-data`](scratch-data) renders structured data in a scratch buffer.
- [`diagnostics-quickfix`](diagnostics-quickfix) filters diagnostics into a navigation list.
- [`selection-format`](selection-format) previews JSON formatting or sorted selection lines.
- [`scratch-refresh`](scratch-refresh) refreshes an existing buffer report.

Run every script inside a Neovim terminal so `$NVIM` identifies the parent editor.
Pass `--server` directly when you run Nushell outside Neovim.

The `agent-control`, `diagnostics-quickfix`, `selection-format`, and `scratch-refresh` recipes resolve the server once before operating.
They use `--server`, then `$NVIM`, then the only discovered editor, and reject ambiguous discovery.
