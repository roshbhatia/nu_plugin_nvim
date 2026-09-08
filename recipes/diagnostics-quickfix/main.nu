use ../common.nu resolve-server

def main [
  --server: string
  --severity: string = "ERROR"
  --source: string
  --title: string = "Diagnostics"
  --open
] {
  let level = ($severity | str uppercase)
  if $level not-in [ERROR WARN INFO HINT] {
    error make {msg: "severity must be ERROR, WARN, INFO, or HINT"}
  }
  let target = (resolve-server $server)
  let entries = (
    nuvim diagnostics --server $target
    | where { |item|
        $item.severity == $level and ($source == null or $item.source? == $source)
      }
    | each { |item|
        {
          path: $item.path
          row: $item.row
          column: $item.column
          text: $item.message
          type: ({ERROR: E, WARN: W, INFO: I, HINT: I} | get $level)
        }
      }
  )
  let result = ($entries | nuvim quickfix set --server $target --title $title)
  if $open {
    nuvim quickfix open --server $target | ignore
  }
  $result | upsert server $target
}
