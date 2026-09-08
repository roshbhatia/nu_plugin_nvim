# Refresh a buffer report

This recipe displays buffer paths, filetypes, and modification flags as NUON.
It excludes its own report buffer and returns that buffer's record.

Create a report:

```nu
nu recipes/scratch-refresh/main.nu --server /path/to/nvim.sock
```

Keep the returned `id` and `server` values, then refresh the same buffer:

```nu
nu recipes/scratch-refresh/main.nu --server /path/to/nvim.sock --buffer 7
```

Replace `7` with the returned buffer ID.
Creation opens the report buffer.
Refresh preserves the current buffer and does not create another report.
Refresh accepts only an existing `nofile` buffer; use the buffer created by this recipe.
The recipe does not save files.

Server selection uses `--server`, then `$NVIM`, then the only discovered editor.
Multiple discovered editors require an explicit choice.
Pass the saved server address when reusing a buffer ID.
