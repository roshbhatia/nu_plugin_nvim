# Open changed Git paths

This recipe sends paths from `git diff --name-only` into Neovim.

```sh
nu recipes/git-diff-open/main.nu
```

Run it from the target Git worktree.
Nuvim loads every path and enters the first buffer.
