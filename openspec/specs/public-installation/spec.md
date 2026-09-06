# public-installation Specification

> The keywords MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY in this document are
> to be interpreted as described in RFC 2119.

## Purpose

Define how users install and register Nuvim without adding an editor-side
Neovim plugin.

## Requirements

### Requirement: Installable Nushell plugin

The flake MUST expose `packages.<system>.nu-plugin`, `runtime`, and `default` as
the `nu_plugin_nuvim` runtime package. The runtime closure MUST NOT require the
maintainer-only code generator.

#### Scenario: User installs from GitHub

- **WHEN** a user installs `github:roshbhatia/nu_plugin_nvim#nu-plugin`
- **THEN** the installed package provides the `nu_plugin_nuvim` executable

### Requirement: Explicit Nushell registration

Users MUST be able to register the installed executable with `plugin add` and
load it with Nushell's normal plugin lifecycle.

#### Scenario: Existing shell reloads the plugin

- **WHEN** the user registers the executable and runs `plugin use nuvim`
- **THEN** the `nuvim` command family becomes available in that shell

### Requirement: No editor-side installation

Nuvim MUST control an existing Neovim server without an editor-side plugin,
native module, or reverse bridge.

#### Scenario: Clean Neovim exposes a socket

- **WHEN** Neovim starts with `--listen` and no Nuvim editor configuration
- **THEN** the Nushell plugin can connect through the advertised server address

### Requirement: Supported release systems

The flake MUST expose packages, checks, formatting, and a development shell for
Apple Silicon macOS, ARM Linux, and x86-64 Linux.

#### Scenario: A supported system evaluates the flake

- **WHEN** Nix evaluates outputs for `aarch64-darwin`, `aarch64-linux`, or `x86_64-linux`
- **THEN** the system has the runtime package and validation outputs

### Requirement: Plugin binary is not a terminal app

The flake MUST NOT expose the Nushell plugin protocol executable as a default
`nix run` application.

#### Scenario: User inspects public flake outputs

- **WHEN** the user distinguishes packages from applications
- **THEN** the plugin is available as a package and not presented as an interactive app
