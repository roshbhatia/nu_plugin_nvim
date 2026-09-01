nuvim diagnostics
| where severity == "ERROR"
| select path row column message source code
