# agent-control Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define the structured observe, navigate, edit, and operate surface that agents
and users use to control an existing Neovim session.

## Requirements

### Requirement: Stable structured command surface

Nuvim MUST provide structured commands for context, cursors, buffers, text,
selections, paths, edits, diagnostics, quickfix lists, scratch buffers, Ex
commands, raw API calls, and Lua evaluation.

#### Scenario: Agent reads editor context

- **WHEN** the agent runs `nuvim context --server <address>`
- **THEN** Nuvim returns the current server, mode, buffer, window, tab, cursor, and working directory as structured data

### Requirement: Explicit session affinity

Every editor command MUST accept `--server`. Automated control SHOULD select a
server first and pass that exact address to every later command.

#### Scenario: Another editor starts during automation

- **WHEN** an agent continues passing its selected server address
- **THEN** its commands remain bound to the original editor

### Requirement: Consistent coordinates

Public rows and columns MUST be zero-based, and columns MUST be UTF-8 byte
offsets. Nuvim MUST perform one-based quickfix conversion only at the Neovim
boundary.

#### Scenario: Agent writes after a multibyte character

- **WHEN** it supplies the correct zero-based UTF-8 byte column
- **THEN** Nuvim edits the intended byte range and reports the same public coordinates

### Requirement: Direct state operations

Navigation and mutation commands MUST use Neovim APIs rather than simulated
keys. Range edits MUST support the current or an explicitly selected buffer and
MUST NOT change the current buffer when editing another buffer.

#### Scenario: Agent edits a non-current buffer

- **WHEN** `nuvim edit` receives another buffer identifier
- **THEN** it changes that buffer while preserving the current buffer

### Requirement: Pipeline-native input and output

Commands MUST accept and return native Nushell values where applicable.
`nuvim open` MUST accept paths from arguments or pipeline input, and scratch and
replacement commands MUST consume pipeline data.

#### Scenario: Git emits changed paths

- **WHEN** a Nushell pipeline passes those paths into `nuvim open`
- **THEN** Nuvim opens them and returns structured buffer records

### Requirement: Mutation remains explicit

Buffer edits MUST remain unsaved until the user or agent explicitly invokes a
write operation. Agent workflows SHOULD inspect before mutation and verify the
affected range afterward.

#### Scenario: Agent replaces a range

- **WHEN** `nuvim edit` succeeds without a later write command
- **THEN** Neovim marks the buffer changed but Nuvim does not persist the file

### Requirement: Escape hatches retain validation

`nuvim call` and `nuvim lua` MUST preserve MessagePack-compatible structured
arguments and results. Agents SHOULD use them only when stable commands cannot
express the required operation.

#### Scenario: Lua returns structured values

- **WHEN** `nuvim lua` returns a table representable as a Nushell record
- **THEN** Nuvim returns the corresponding native structured value
