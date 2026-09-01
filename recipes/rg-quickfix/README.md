# Send TODO matches to quickfix

This recipe converts ripgrep JSON events into the Nuvim quickfix schema.

```sh
nu recipes/rg-quickfix/main.nu
```

Ripgrep reports one-based line numbers and zero-based byte offsets.
The recipe subtracts one from each line number before Nuvim receives it.
