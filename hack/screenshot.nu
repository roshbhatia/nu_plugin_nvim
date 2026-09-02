nuvim buffers
| update path { |row| $row.path | path relative-to $env.NUVIM_SCREENSHOT_REPO }
| select id path filetype modified
