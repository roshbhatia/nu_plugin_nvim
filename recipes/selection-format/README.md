# Preview formatted selection text

This recipe formats the last visual selection into a new scratch buffer.
It leaves the source buffer unchanged and returns the source selection and preview buffer records.

Make a characterwise or linewise selection, then leave Visual mode before running:

```nu
nu recipes/selection-format/main.nu --format json
nu recipes/selection-format/main.nu --server /path/to/nvim.sock --format sort
```

`json` is the default format and pretty-prints selected JSON through Nushell.
`sort` sorts the selected lines.
Invalid JSON fails before creating a preview.
Each successful run opens a new unnamed scratch buffer.
Blockwise selections remain unsupported.

Server selection uses `--server`, then `$NVIM`, then the only discovered editor.
Multiple discovered editors require an explicit choice.
The recipe does not apply the preview or save files.
