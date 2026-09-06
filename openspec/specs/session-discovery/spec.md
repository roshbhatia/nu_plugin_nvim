# session-discovery Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define how Nuvim discovers, identifies, selects, and binds live Neovim server
sessions.

## Requirements

### Requirement: Server selection precedence

A command-specific `--server` value MUST take precedence over `$NVIM`. When
neither exists, a command MUST discover standard runtime sockets.

#### Scenario: Flag and environment disagree

- **WHEN** `--server` and `$NVIM` name different servers
- **THEN** Nuvim targets the `--server` value

### Requirement: Live socket discovery

Discovery MUST search the standard XDG runtime and temporary Neovim socket
locations, deduplicate candidates, order them newest first, and retain only
servers that answer a bounded `nvim_get_api_info` probe.

#### Scenario: Runtime directory contains a stale socket

- **WHEN** one candidate cannot answer the bounded Neovim probe
- **THEN** Nuvim omits that candidate from selection

### Requirement: Deterministic automatic selection

Commands MUST automatically use the only discovered live server. They MUST
return an actionable error when none exist and MUST require explicit selection
when several exist.

#### Scenario: One editor is live

- **WHEN** neither `--server` nor `$NVIM` is set and discovery finds one server
- **THEN** an editor command targets that server without prompting

#### Scenario: Several editors are live

- **WHEN** an editor command cannot identify one of several discovered servers
- **THEN** it tells the user to run bare `nuvim` or pass `--server`

### Requirement: Interactive root picker

Bare `nuvim` MUST return the sole live session or use Nushell's native table
picker when several sessions are live. A cancelled selection MUST return an
explicit error.

#### Scenario: User chooses one of several editors

- **WHEN** the user selects a row from bare `nuvim`
- **THEN** Nuvim returns the complete record for that exact server

### Requirement: Inspectable session identity

Session records MUST include a label, server address, process identifier,
working directory, current buffer path, and mode. Labels SHOULD use the current
buffer name, then the working-directory name, then a Neovim fallback.

#### Scenario: Current buffer has a path

- **WHEN** Nuvim describes a live server with a named current buffer
- **THEN** the session label uses that buffer's file name

### Requirement: Non-interactive discovery

`nuvim servers` MUST return all live session records without prompting so that
automation can select a server and pass it explicitly to later commands.

#### Scenario: Agent needs stable affinity

- **WHEN** an agent selects one record from `nuvim servers`
- **THEN** it can preserve affinity by passing that record's server to every command
