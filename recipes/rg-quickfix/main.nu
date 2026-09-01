rg TODO --json
| lines
| each { $in | from json }
| where type == "match"
| each { |event|
    {
      path: $event.data.path.text
      row: ($event.data.line_number - 1)
      column: $event.data.submatches.0.start
      text: ($event.data.lines.text | str trim --right)
      type: "I"
    }
  }
| nuvim quickfix set --title "TODO"
| nuvim quickfix open
