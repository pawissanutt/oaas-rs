# OPRC CLI - Oparaca Command Line Interface

A comprehensive command-line interface for the Oparaca (OaaS) platform, providing both low-level object operations and high-level package management capabilities.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Configuration](#configuration)
- [Commands](#commands)
  - [Context Management](#context-management)
  - [Package Management](#package-management)
  - [Class Management](#class-management)
  - [Function Management](#function-management)
  - [Deployment Management](#deployment-management)
  - [Runtime Management](#runtime-management)
  - [Object Operations](#object-operations)
  - [Invocation Operations](#invocation-operations)
  - [Result Operations](#result-operations)
  - [Liveliness Operations](#liveliness-operations)
- [Output Formats](#output-formats)
- [Examples](#examples)
- [Migration Guide](#migration-guide)
- [Troubleshooting](#troubleshooting)

## Overview

OPRC CLI integrates OCLI (Oparaca CLI) functionality with the existing OPRC commands, providing a unified interface for:

- **Package Management** - Deploy and manage application packages
- **Class Management** - List and manage deployed classes
- **Function Management** - List and invoke functions
- **Context Management** - Configure and switch between environments
- **Deployment Management** - Monitor and manage deployments
- **Runtime Management** - Manage class runtime instances
- **Object Operations** - Low-level object CRUD operations (existing)
- **Invocation Operations** - Direct function invocation (existing)

## Installation

### Install with Cargo

```bash
cargo install --bin oprc-cli
```

### Add to PATH

```bash
# Linux/macOS
export PATH="$PATH:$HOME/.cargo/bin"

# Windows (PowerShell)
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
```

## Configuration

OPRC CLI uses a context-based configuration system that stores settings in `$HOME/.oprc/config.yml`.

### Configuration File Structure

```yaml
contexts:
  default:
    pmUrl: "http://pm.oaas.127.0.0.1.nip.io"
    gatewayUrl: "http://oaas.127.0.0.1.nip.io"
    defaultClass: "example.record"
    zenohPeer: "tcp/127.0.0.1:7447"
  production:
    pmUrl: "https://pm.prod.example.com"
    gatewayUrl: "https://api.prod.example.com"
    defaultClass: "prod.main"
    zenohPeer: "tcp/prod.zenoh:7447"
currentContext: "default"
```

### Environment Variables

- `OPRC_CONFIG_PATH` - Override the default config file location
- `OPRC_LOG` - Set logging level (existing)

## Commands

### Context Management

Manage configuration contexts for different environments.

```bash
# Set context configuration
oprc-cli context set [OPTIONS]
oprc-cli ctx s [OPTIONS]  # Short alias

# Display current configuration
oprc-cli context get
oprc-cli ctx g  # Short alias

# Switch between contexts
oprc-cli context select <CONTEXT_NAME>
```

#### Context Set Options

```bash
oprc-cli context set \
  --name production \
  --pm "https://pm.prod.example.com" \
  --gateway "https://api.prod.example.com" \
  --cls "prod.main" \
  --zenoh-peer "tcp/prod.zenoh:7447"
```

- `--name` - Context name (defaults to current)
- `--pm` - Package manager URL
- `--gateway` - Gateway URL for API access
- `--cls` - Default class name
- `--zenoh-peer` - Zenoh peer endpoint

### Package Management

Deploy and manage application packages using YAML definitions.

```bash
# Deploy/update a package
oprc-cli package apply <YAML_FILE> [OPTIONS]
oprc-cli pkg a <YAML_FILE> [OPTIONS]  # Short alias

# Delete a package
oprc-cli package delete <YAML_FILE> [OPTIONS]
oprc-cli pkg d <YAML_FILE> [OPTIONS]  # Short alias
```

#### Package Apply/Delete Options

- `-p, --override-package <NAME>` - Override package name from YAML



### Class Management

List and manage deployed classes.

```bash
# List all classes
oprc-cli class list [CLASS_NAME]
oprc-cli cls l [CLASS_NAME]  # Short alias

# Delete a class
oprc-cli class delete <CLASS_NAME>
oprc-cli cls delete <CLASS_NAME>  # Short alias
```

### Function Management

List available functions.

```bash
# List all functions
oprc-cli function list [FUNCTION_NAME]
oprc-cli fn l [FUNCTION_NAME]  # Short alias
```

### Deployment Management

Monitor and manage deployments.

```bash
# List deployments
oprc-cli deploy list [DEPLOYMENT_NAME]
oprc-cli dep l [DEPLOYMENT_NAME]  # Short alias

# Delete a deployment
oprc-cli deploy delete <DEPLOYMENT_NAME>
oprc-cli dep delete <DEPLOYMENT_NAME>  # Short alias
```

### Runtime Management

Manage class runtime instances.

```bash
# List class runtimes
oprc-cli class-runtime list [RUNTIME_NAME]
oprc-cli cr l [RUNTIME_NAME]  # Short alias

# Delete a runtime
oprc-cli class-runtime delete <RUNTIME_NAME>
oprc-cli cr delete <RUNTIME_NAME>  # Short alias
```

### Object Operations

Low-level object CRUD operations (existing functionality).

```bash
# Object operations
oprc-cli object <OPERATION>
oprc-cli obj <OPERATION>  # Short alias
oprc-cli o <OPERATION>    # Shorter alias
```

### Invocation Operations

Direct function invocation (existing functionality).

```bash
# Invoke functions
oprc-cli invoke <OPTIONS>
oprc-cli ivk <OPTIONS>  # Short alias
oprc-cli i <OPTIONS>    # Shorter alias
```

### Result Operations

Retrieve async operation results (existing functionality).

```bash
# Get results
oprc-cli result <OPTIONS>
oprc-cli res <OPTIONS>  # Short alias
oprc-cli r <OPTIONS>    # Shorter alias
```

### Liveliness Operations

Check cluster liveliness (existing functionality).

```bash
# Check liveliness
oprc-cli liveliness
oprc-cli l  # Short alias
```

## Output Formats

OPRC CLI supports multiple output formats for data presentation:

- **JSON** (default) - Machine-readable format
- **YAML** - Human-readable format  
- **Table** - Formatted table output

```bash
# Examples with different output formats
oprc-cli class list -o json   # JSON output (default)
oprc-cli class list -o yaml   # YAML output
oprc-cli class list -o table  # Table output
```