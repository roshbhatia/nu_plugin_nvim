local namespace = vim.api.nvim_create_namespace("nuvim-demo")

vim.diagnostic.set(namespace, 0, {
  {
    lnum = 2,
    col = 0,
    severity = vim.diagnostic.severity.ERROR,
    message = "Introduction needs a concrete example",
  },
  {
    lnum = 7,
    col = 0,
    severity = vim.diagnostic.severity.WARN,
    message = "Architecture link should name the RPC boundary",
  },
})
