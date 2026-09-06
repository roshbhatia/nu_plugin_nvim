# documentation-media-release Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define the public evidence, runnable examples, validation, and release contract
for Nuvim.

## Requirements

### Requirement: Visual evidence leads the README

The README MUST show the current screenshot and animated demonstration directly
after its title. Both artifacts MUST demonstrate the public `nuvim` command.

#### Scenario: User opens the repository

- **WHEN** the README renders on GitHub
- **THEN** the screenshot and animation appear before installation and command details

### Requirement: Reproducible media

Repository scripts MUST generate the screenshot and animation through the
packaged plugin. A recorded fingerprint MUST cover the media inputs, and the
media check MUST fail when generated artifacts are stale.

#### Scenario: Demo script changes

- **WHEN** a media input changes without regenerating the published artifacts
- **THEN** the media check exits unsuccessfully and requests regeneration

### Requirement: Runnable workflow recipes

Each published recipe MUST live in its own directory with a focused README and
Nushell script. Examples MUST use real `nuvim` commands and native pipeline
data rather than a demo-only executable.

#### Scenario: User follows the agent-control recipe

- **WHEN** the recipe runs against a selected Neovim server
- **THEN** it demonstrates the supported inspect, navigate, edit, and verify flow

### Requirement: Public architecture boundaries

Documentation MUST describe the direct MessagePack-RPC architecture, generated
client, session selection rules, coordinate model, and agent-control contract.
It MUST identify expose handlers and event watches as deferred behavior.

#### Scenario: User evaluates current callback support

- **WHEN** the user reads the architecture documentation
- **THEN** it does not claim that `nuvim expose` or `nuvim watch` is shipped

### Requirement: Repository validation

CI MUST check formatting, Clippy with warnings denied, generated-client drift,
workflow syntax, runtime behavior, session discovery, reverse-bridge absence,
and media freshness through the Nix flake checks.

#### Scenario: Pull request changes public behavior

- **WHEN** CI runs on a supported pull request
- **THEN** the repository checks exercise the generated and runtime contracts

### Requirement: Multi-platform release gate

A release tag MUST match the Cargo package version. Release automation MUST run
the flake checks on Apple Silicon macOS, ARM Linux, and x86-64 Linux before it
creates or updates the verified GitHub source release.

#### Scenario: Tag does not match package version

- **WHEN** release automation receives a mismatched version tag
- **THEN** it fails before publishing a GitHub release

### Requirement: Automated source updates

Automation MUST update Cargo dependencies and every root or nested flake lock
on a six-hour schedule or manual dispatch. It MUST regenerate derived sources
and media before it opens a pull request. It MUST test the exact pull request
head on Apple Silicon macOS, ARM Linux, and x86-64 Linux before merging that
same head.

#### Scenario: Dependency update changes generated content

- **WHEN** scheduled automation updates a dependency or flake input
- **THEN** the pull request includes regenerated clients, hashes, and media
- **AND** it merges only after the exact head passes all three platform checks
