# List modified buffers

This recipe treats buffers as records and keeps only unsaved files.

```sh
nu recipes/modified-buffers/main.nu
```

The output contains buffer ID, path, filetype, and changed tick.
