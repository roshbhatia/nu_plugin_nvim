# generated-rpc-client Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define how Nuvim derives its typed Rust RPC surface from Neovim's own API
metadata.

## Requirements

### Requirement: Neovim API is the generation source

`nuvim-codegen` MUST read `nvim --api-info` and generate API version constants,
function metadata, and one Rust client method for each reported function.

#### Scenario: Neovim reports an API function

- **WHEN** code generation reads that function from the API metadata
- **THEN** the generated Rust source contains its metadata and client method

### Requirement: Deterministic checked-in client

Generation MUST produce deterministic formatted Rust for the same API metadata.
The repository MUST track the generated source so runtime builds do not execute
the generator.

#### Scenario: Generation repeats without API drift

- **WHEN** the generator runs twice against the same Neovim API
- **THEN** both runs produce identical `generated.rs` content

### Requirement: Generation rejects ambiguous identifiers

The generator MUST normalize API parameter names into Rust identifiers and MUST
reject duplicate identifiers created by normalization.

#### Scenario: Two parameters normalize to one Rust name

- **WHEN** an API function would generate duplicate parameter identifiers
- **THEN** generation fails instead of emitting ambiguous Rust

### Requirement: Generated metadata guards raw calls

`nuvim call` MUST reject an unknown method and MUST validate argument count
against generated metadata before sending a request.

#### Scenario: Raw call has the wrong arity

- **WHEN** a user supplies fewer or more arguments than the generated signature
- **THEN** Nuvim reports the expected and received counts without calling Neovim

### Requirement: Drift check is non-mutating

`nuvim-codegen --check` MUST fail when the checked-in client differs from fresh
generation and MUST NOT rewrite the tracked file.

#### Scenario: Checked-in client is stale

- **WHEN** the Neovim API metadata changes without regeneration
- **THEN** the code-generation check exits unsuccessfully and names the stale file
