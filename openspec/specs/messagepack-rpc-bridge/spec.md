# messagepack-rpc-bridge Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define the one-way Nushell-to-Neovim control path and its lossless structured
value boundary.

## Requirements

### Requirement: Direct MessagePack-RPC transport

Each editor command MUST connect from Nushell to an existing Neovim Unix socket
or TCP server, issue MessagePack-RPC requests, and close without requiring a
resident editor-side component.

#### Scenario: Command targets a TCP server

- **WHEN** a command receives a valid `host:port` server address
- **THEN** it performs the requested Neovim RPC over that TCP connection

#### Scenario: Command targets a Unix socket

- **WHEN** a command receives a filesystem server address
- **THEN** it performs the requested Neovim RPC over that Unix socket

### Requirement: Bounded and contextual RPC failure

Connections, reads, and writes MUST use bounded timeouts. Transport, protocol,
and remote failures MUST identify the target server and MUST NOT panic.

#### Scenario: Server does not answer

- **WHEN** an RPC call exceeds its transport timeout
- **THEN** Nuvim returns a contextual error instead of waiting without a bound

### Requirement: Protocol-safe response handling

Requests MUST use unique increasing identifiers. The client MUST retain
notifications received before the matching response and MUST reject malformed
messages or a response for another request identifier.

#### Scenario: Notification precedes a response

- **WHEN** Neovim emits a notification before the response to the active request
- **THEN** the client queues the notification and continues to the matching response

### Requirement: Structured value preservation

Nuvim MUST convert ordinary MessagePack values to native Nushell values.
Unknown extensions, out-of-range unsigned integers, and maps with non-string
keys MUST use explicit tagged records rather than lose data.

#### Scenario: Neovim returns a map with a non-string key

- **WHEN** the RPC result cannot be represented as a Nushell record
- **THEN** Nuvim returns a tagged MessagePack map with explicit key and value entries

### Requirement: Server-bound handles

Buffer, window, and tab handles MUST retain their kind, identifier, and server.
Nuvim MUST reject a handle for another server, including one nested in a list,
record, or pipeline value.

#### Scenario: Handle crosses editor sessions

- **WHEN** a command targeting one server receives a handle created by another
- **THEN** Nuvim rejects the request before sending it to Neovim

### Requirement: One-way bridge scope

The current interface MUST NOT expose Nushell handlers to Neovim and MUST NOT
stream Neovim events into Nushell. `nuvim expose` and `nuvim watch` remain
future behavior outside this baseline.

#### Scenario: Public commands are enumerated

- **WHEN** a user inspects the registered Nuvim commands
- **THEN** no expose or watch command is present
