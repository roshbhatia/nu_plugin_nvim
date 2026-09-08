export def resolve-server [server?: string] {
  if $server == null {
    nuvim context | get server
  } else {
    nuvim context --server $server | get server
  }
}
