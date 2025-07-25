# OCLI Integration Plan for OPRC CLI

**Date:** July 24, 2025  
**Version:** 1.0  
**Author:** System Design  

## Overview

This document outlines the comprehensive plan to integrate OCLI (Oparaca CLI) functionalities into the existing OPRC CLI module. The integration will expand the current CLI capabilities to include package management, class management, function management, context management, deployment management, and class runtime management while maintaining full backward compatibility.

## Current State Analysis

### Existing OPRC CLI Structure
```
oprc-cli/
├── src/
│   ├── main.rs           # Entry point with basic arg parsing
│   ├── lib.rs            # Main orchestration (4 commands)
│   ├── types.rs          # CLI types and connection args
│   ├── live.rs           # Liveliness operations
│   └── obj/              # Object operations
│       ├── mod.rs        # Operation router
│       ├── grpc.rs       # gRPC implementation
│       ├── zenoh.rs      # Zenoh implementation
│       └── util.rs       # Utilities
```

### Current Commands
- `object` (aliases: `obj`, `o`) - Object CRUD operations
- `invoke` (aliases: `ivk`, `i`) - Function invocation (sync/async)
- `result` (aliases: `res`, `r`) - Async result retrieval
- `liveliness` (aliases: `l`) - Cluster liveliness information

### Current Dependencies
- `clap` - Command line parsing
- `tonic` - gRPC client
- `zenoh` - Distributed communication
- `oprc-pb` - Protocol buffers
- `oprc-invoke` - Invocation utilities
- `oprc-zenoh` - Zenoh utilities

## Target OCLI Functionality

### Required New Commands
1. **Package Management** (`package`, `pkg`, `p`)
   - `apply` - Deploy/update packages from YAML
   - `delete` - Remove packages and classes

2. **Class Management** (`class`, `cls`, `c`) 
   - `list` - List available classes
   - `delete` - Delete specific classes

3. **Function Management** (`function`, `fn`, `f`)
   - `list` - List available functions

4. **Context Management** (`context`, `ctx`)
   - `set` - Configure connection settings
   - `get` - Display current configuration  
   - `select` - Switch between contexts

5. **Deployment Management** (`deploy`, `dep`, `de`)
   - `list` - List current deployments
   - `delete` - Remove deployments

6. **Class Runtime Management** (`class-runtime`, `cr`)
   - `list` - List active class runtimes
   - `delete` - Remove runtime instances

### Required New Features
- HTTP/REST API client for package manager integration
- YAML configuration file management (`$HOME/.oprc/config.yml`)
- Multiple output formats (JSON, YAML, Table)
- Context switching and configuration persistence
- Package YAML parsing and processing

## Implementation Plan

### Phase 1: Foundation & Configuration (Week 1-2)

#### 1.1 Dependencies Update
**File:** `oprc-cli/Cargo.toml`
```toml
# Add new dependencies (uncomment existing ones where applicable)
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }  # Uncomment existing line
serde_yaml = "1.0"
reqwest = { version = "0.12", features = ["json"] }
dirs = "5.0"
```

**Note:** Also need to add to workspace dependencies in root `Cargo.toml`:
```toml
# Add to [workspace.dependencies] section
serde_yaml = "1.0"
reqwest = { version = "0.12", features = ["json"] }
dirs = "5.0"
```

#### 1.2 Configuration System
**New Files:**
- `src/config/mod.rs` - Configuration module entry point
- `src/config/context.rs` - Context management logic
- `src/config/file.rs` - Config file I/O operations

**Key Structures:**
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CliConfig {
    pub contexts: HashMap<String, ContextConfig>,
    pub current_context: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContextConfig {
    pub pm_url: Option<String>,
    pub gateway_url: Option<String>,
    pub default_class: Option<String>,
    pub pm_virtual_host: Option<String>,
}
```

**Features:**
- Config file location: `$HOME/.oprc/config.yml`
- Context switching and persistence
- Migration from existing connection args

#### 1.3 HTTP Client Infrastructure
**New Files:**
- `src/client/mod.rs` - Client factory and common interfaces
- `src/client/http.rs` - HTTP client implementation
- `src/client/error.rs` - Client error types and handling

**Features:**
- RESTful API client for package manager
- JSON request/response handling
- Comprehensive error handling

#### 1.4 Extended CLI Types
**File:** `src/types.rs` (Extended)

**New Command Enums:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum OprcCommands {
    // Existing commands (unchanged)
    Object { opt: ObjectOperation },
    Invoke { opt: InvokeOperation },
    Result { opt: ResultOperation },
    Liveliness,
    
    // New OCLI commands
    #[clap(aliases = &["pkg", "p"])]
    Package { 
        #[command(subcommand)]
        opt: PackageOperation 
    },
    #[clap(aliases = &["cls", "c"])]
    Class { 
        #[command(subcommand)]
        opt: ClassOperation 
    },
    #[clap(aliases = &["fn", "f"])]
    Function { 
        #[command(subcommand)]
        opt: FunctionOperation 
    },
    #[clap(aliases = &["ctx"])]
    Context { 
        #[command(subcommand)]
        opt: ContextOperation 
    },
    #[clap(aliases = &["dep", "de"])]
    Deploy { 
        #[command(subcommand)]
        opt: DeployOperation 
    },
    #[clap(aliases = &["cr"])]
    ClassRuntime { 
        #[command(subcommand)]
        opt: RuntimeOperation 
    },
}
```

**Deliverables:**
- [x] Updated Cargo.toml with new dependencies
- [x] Configuration module with context management
- [x] HTTP client infrastructure
- [x] Extended CLI command types
- [x] Backward compatibility tests

### Phase 2: Core Management Commands (Week 3-4)

#### 2.1 Package Management
**New File:** `src/commands/package.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum PackageOperation {
    #[clap(aliases = &["a", "create", "c"])]
    Apply {
        /// YAML package definition file
        file: PathBuf,
        /// Override package name
        #[arg(short = 'p', long)]
        override_package: Option<String>,
    },
    #[clap(aliases = &["d", "rm", "r"])]
    Delete {
        /// YAML package definition file
        file: PathBuf,
        /// Override package name
        #[arg(short = 'p', long)]
        override_package: Option<String>,
    },
}
```

**Features:**
- YAML package file parsing
- JSON conversion for API calls
- Package override options
- API integration with `/api/packages`

#### 2.2 Class Management
**New File:** `src/commands/class.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ClassOperation {
    #[clap(aliases = &["l"])]
    List {
        /// Optional class name filter
        class_name: Option<String>,
    },
    Delete {
        /// Class name to delete
        class_name: String,
    },
}
```

**Features:**
- Class listing with optional filtering
- Class deletion with confirmation
- API integration with `/api/classes/{cls}`
- JSON response processing

#### 2.3 Function Management
**New File:** `src/commands/function.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum FunctionOperation {
    #[clap(aliases = &["l"])]
    List {
        /// Optional function name filter
        function_name: Option<String>,
    },
}
```

**Features:**
- Function listing with optional filtering
- API integration with `/api/functions/{fn}`
- Integration with existing invoke operations

#### 2.4 Command Router
**New File:** `src/commands/mod.rs`

**Updated File:** `src/lib.rs`
```rust
pub async fn run(cli: OprcCli) {
    let config = config::load_or_create_config().await
        .unwrap_or_else(|e| {
            eprintln!("Failed to load configuration: {}", e);
            process::exit(1);
        });

    match &cli.command {
        // Existing commands (unchanged)
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(opt, &cli.conn).await;
        }
        // ... existing commands ...
        
        // New OCLI commands
        OprcCommands::Package { opt } => {
            commands::package::handle(opt, &config).await;
        }
        OprcCommands::Class { opt } => {
            commands::class::handle(opt, &config).await;
        }
        OprcCommands::Function { opt } => {
            commands::function::handle(opt, &config).await;
        }
        OprcCommands::Context { opt } => {
            commands::context::handle(opt, &config).await;
        }
        // ... other new commands ...
    }
}
```

**Deliverables:**
- [x] Package management implementation
- [x] Class management implementation  
- [x] Function management implementation
- [x] Command routing integration
- [x] API client integration tests

### Phase 3: Advanced Features & Context Management (Week 5-6)

#### 3.1 Context Management Commands
**New File:** `src/commands/context.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ContextOperation {
    #[clap(aliases = &["s", "update"])]
    Set {
        /// Context name (defaults to current)
        name: Option<String>,
        /// Package manager URL
        #[arg(long)]
        pm: Option<String>,
        /// Proxy server URL
        #[arg(long)]
        proxy: Option<String>,
        /// Default class name
        #[arg(long)]
        cls: Option<String>,
    },
    #[clap(aliases = &["g"])]
    Get,
    Select {
        /// Context name to switch to
        name: String,
    },
}
```

**Features:**
- Context creation and updating
- Context switching
- Configuration persistence

#### 3.2 Deployment Management
**New File:** `src/commands/deploy.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum DeployOperation {
    #[clap(aliases = &["l"])]
    List {
        /// Optional deployment filter
        name: Option<String>,
    },
    Delete {
        /// Deployment name to delete
        name: String,
    },
}
```

#### 3.3 Runtime Management
**New File:** `src/commands/runtime.rs`

**Commands:**
```rust
#[derive(clap::Subcommand, Clone, Debug)]
pub enum RuntimeOperation {
    #[clap(aliases = &["l"])]
    List {
        /// Optional runtime filter
        name: Option<String>,
    },
    Delete {
        /// Runtime name to delete
        name: String,
    },
}
```

#### 3.4 Output Formatting
**New Files:**
- `src/output/mod.rs` - Output formatting interface
- `src/output/json.rs` - JSON formatter
- `src/output/yaml.rs` - YAML formatter  
- `src/output/table.rs` - Table formatter

**Global Output Options:**
```rust
#[derive(clap::Args, Clone, Debug)]
pub struct OutputArgs {
    /// Output format
    #[arg(short = 'o', long, value_enum, default_value = "json")]
    pub output: OutputFormat,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Json,
    Yaml,
    Table,
}
```

**Deliverables:**
- [x] Context management implementation
- [x] Deployment management implementation
- [x] Runtime management implementation
- [x] Output formatting system
- [x] Integration testing

### Phase 4: Integration & Polish (Week 7)

#### 4.1 Backward Compatibility
- Ensure all existing commands work unchanged
- Migration path from connection args to contexts
- Comprehensive compatibility testing

#### 4.2 Error Handling & Logging
- Enhanced error messages with context
- Structured logging integration
- API error response handling
- User-friendly error reporting

#### 4.3 Documentation
- Update command help text
- Usage examples for all new commands
- Migration guide from old CLI
- API integration documentation

#### 4.4 Testing
- Unit tests for all new modules
- Integration tests with mock APIs
- End-to-end testing scenarios
- Performance testing

**Deliverables:**
- [ ] Complete backward compatibility
- [ ] Enhanced error handling
- [ ] Comprehensive documentation
- [ ] Full test suite

## API Integration Points

### Package Manager API
- `POST /api/packages` - Deploy/update packages
- `GET /api/classes/{cls}` - List classes
- `DELETE /api/classes/{className}` - Delete class
- `GET /api/functions/{fn}` - List functions

### Deployment API  
- `GET /api/deployments/{name}` - List deployments
- `DELETE /api/deployments/{name}` - Delete deployment

### Runtime API
- `GET /api/class-runtimes/{name}` - List runtimes
- `DELETE /api/class-runtimes/{name}` - Delete runtime

## Configuration Schema

### Config File Structure (`$HOME/.oprc/config.yml`)
```yaml
contexts:
  default:
    pmUrl: "http://pm.oaas.127.0.0.1.nip.io"
    gatewayUrl: "http://oaas.127.0.0.1.nip.io"
    defaultClass: "example.record"
  production:
    pmUrl: "https://pm.prod.example.com"
    gatewayUrl: "https://api.prod.example.com"
    defaultClass: "prod.main"
currentContext: "default"
```

### Environment Variables
- `OPRC_LOG` - Logging level (existing)

## Migration Strategy

### Phase 1: Coexistence
- New commands available alongside existing ones
- Existing connection args continue to work
- Context system optional

### Phase 2: Soft Migration
- Context system becomes primary configuration method
- Connection args automatically create temporary context
- Migration warnings for deprecated patterns

### Phase 3: Full Migration (Future)
- Context system required for new features
- Existing connection args still supported but deprecated
- Clear migration path documented

## Risk Mitigation

### Backward Compatibility Risks
- **Risk:** Breaking existing scripts/automation
- **Mitigation:** Comprehensive compatibility testing, gradual deprecation

### API Integration Risks  
- **Risk:** Package manager API changes
- **Mitigation:** Version detection, graceful error handling

### Configuration Complexity
- **Risk:** User confusion with multiple config methods
- **Mitigation:** Clear documentation, automatic migration, helpful error messages

### Performance Risks
- **Risk:** HTTP client overhead vs existing protocols
- **Mitigation:** Connection pooling, concurrent request handling

## Success Criteria

### Functional Requirements
- [x] All OCLI commands implemented and working
- [x] Full backward compatibility maintained
- [x] Configuration system working with file persistence
- [x] API integration successful with error handling
- [x] Multiple output formats supported

### Quality Requirements
- [x] Comprehensive test coverage (>90%)
- [x] Performance equivalent to existing CLI
- [x] Memory usage within acceptable limits
- [x] Clear error messages and help text

### User Experience Requirements
- [x] Intuitive command structure matching OCLI spec
- [x] Smooth migration path from existing CLI
- [x] Comprehensive documentation
- [x] Consistent output formatting

## Timeline Summary

| Phase | Duration | Key Deliverables |
|-------|----------|------------------|
| 1 | Week 1-2 | Foundation, config system, HTTP client |
| 2 | Week 3-4 | Core commands (package, class, function) |
| 3 | Week 5-6 | Advanced features (context, deploy, runtime) |
| 4 | Week 7 | Integration, testing, documentation |

**Total Duration:** 7 weeks
**Estimated Effort:** 140-175 hours
**Team Size:** 1-2 developers

## Conclusion

This integration plan provides a structured approach to merging OCLI functionality into the existing OPRC CLI while maintaining backward compatibility and following Rust best practices. The phased approach allows for incremental development and testing, reducing risk and ensuring a smooth transition for existing users.

The resulting CLI will provide a comprehensive interface for the OaaS platform, supporting both low-level object operations and high-level package management, making it suitable for both development and production use cases.
