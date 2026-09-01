git diff --name-only
| lines
| where { not ($in | is-empty) }
| nuvim open
