use ../common.nu resolve-server

def main [--server: string, --format: string = "json"] {
  if $format not-in [json sort] {
    error make {msg: "format must be json or sort"}
  }
  let target = (resolve-server $server)
  let selection = (nuvim selection --server $target)
  let formatted = match $format {
    json => ($selection.text | from json | to json)
    sort => ($selection.text | lines | sort | str join "\n")
  }
  let filetype = if $format == "json" { "json" } else { "text" }
  let preview = ($formatted | nuvim scratch --server $target --filetype $filetype)
  {server: $target, source: $selection, preview: $preview}
}
