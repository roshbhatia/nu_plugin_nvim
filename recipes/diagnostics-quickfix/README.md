# Send diagnostics to quickfix

This recipe filters editor diagnostics and replaces the current quickfix list.
It returns the server, entry count, and list title.

```nu
nu recipes/diagnostics-quickfix/main.nu --severity ERROR --open
nu recipes/diagnostics-quickfix/main.nu --server /path/to/nvim.sock --source rustc --severity WARN
```

Severity defaults to `ERROR` and accepts `ERROR`, `WARN`, `INFO`, or `HINT`, regardless of case.
`--source` matches the diagnostic source exactly.
`--title` defaults to `Diagnostics`.
An empty result clears the current list.
The recipe opens the quickfix window only with `--open`.
Rows and columns remain zero-based byte positions at the Nuvim boundary.

Server selection uses `--server`, then `$NVIM`, then the only discovered editor.
Multiple discovered editors require an explicit choice.
