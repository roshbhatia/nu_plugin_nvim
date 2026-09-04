let context = (nuvim context)
let buffers = (
  nuvim buffers
  | update path { |row| $row.path | path relative-to $env.NUVIM_SCREENSHOT_REPO }
  | select id path filetype modified
)
let diagnostics = (
  nuvim diagnostics
  | update path { |row| $row.path | path relative-to $env.NUVIM_SCREENSHOT_REPO }
  | select severity path row column message
)

print $"(ansi cyan_bold)Nuvim(ansi reset)  direct Neovim RPC as Nushell data"
print $"(ansi dark_gray)mode ($context.mode) · cwd ($context.cwd | path basename) · cursor ($context.cursor.row):($context.cursor.column)(ansi reset)"
print ""
print $"(ansi purple_bold)buffers(ansi reset)"
print ($buffers | table)
print $"(ansi purple_bold)diagnostics(ansi reset)"
print ($diagnostics | table)
