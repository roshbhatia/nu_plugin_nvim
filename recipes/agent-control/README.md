# Control Neovim from an agent

This recipe demonstrates the stable agent control contract. It creates a
scratch buffer, moves the cursor, inserts text through a range edit, and reads
the result back as structured Nushell data.

Run it while one Neovim server is available:

```nu
nu recipes/agent-control/main.nu
```

Select a specific editor when several are running:

```nu
nu recipes/agent-control/main.nu --server /path/to/nvim.sock
```

The script does not write a file. See the full
[agent control contract](../../docs/agent-control.md) before giving these
commands to an automated agent.
