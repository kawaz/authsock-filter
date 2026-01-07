# authsock-filter Project Review Context

This file provides project-specific review perspectives for `/thorough-review`.

## Project Overview

A CLI tool (Rust) that acts as an SSH Agent proxy with filtering capabilities.

## Security Review Focus

### SSH Agent Protocol
- Parsing in `src/protocol/`
- Buffer overflow, integer overflow, malformed message handling
- Response validation from upstream SSH Agent (trust boundary)

### Unix Sockets
- Socket creation in `src/agent/server.rs`
- Permission settings (prevent access from other users)
- Symlink attacks

### Paths & Environment Variables
- Injection via `shellexpand` usage
- Path traversal
- `src/utils/path.rs`

### Configuration Files
- TOML parsing in `src/config/mod.rs`, `file.rs`

## Architecture Review Focus

### Main Modules
- `src/agent/` - Proxy implementation (server, proxy, upstream)
- `src/filter/` - Filter evaluation
- `src/protocol/` - SSH Agent protocol
- `src/cli/` - CLI handling
- `src/config/` - Configuration management

### CLI/Config Consistency
- Round-trip conversion: CLI args → Config → CLI must preserve information
- `src/cli/args.rs`, `src/config/mod.rs`
- Filter format: `Vec<Vec<String>>` (outer=OR, inner=AND)
- Merge logic for `--socket` with same path

## Error Handling Focus

### Inode Monitoring
- Robustness of monitoring in `src/cli/commands/run.rs`

### Connection Management
- Timeout for upstream SSH Agent connections
- `src/agent/upstream.rs`

## UX Review Focus

### For Beginners
- Explanation for users unfamiliar with SSH Agent
- Filter "AND/OR" concept
- "upstream" terminology

### For Experts
- launchd/systemd integration instructions
- Configuration file portability

## Test Coverage Focus

### Priority Test Targets
- Protocol parsing (malformed messages)
- Filter evaluation logic
- Inode monitoring
- Behavior on upstream Agent disconnection

## Key Files

```
src/
├── lib.rs              # Module structure
├── error.rs            # Error types
├── agent/
│   ├── server.rs       # Socket server
│   ├── proxy.rs        # Proxy handling
│   └── upstream.rs     # Upstream Agent connection
├── protocol/
│   ├── mod.rs
│   ├── message.rs      # Message types
│   └── codec.rs        # Encode/decode
├── filter/
│   ├── mod.rs          # Filter evaluation
│   ├── github.rs       # GitHub API integration
│   └── keyfile.rs      # Key files
├── config/
│   ├── mod.rs
│   └── file.rs         # Configuration file
├── cli/
│   ├── mod.rs
│   ├── args.rs         # CLI argument definitions
│   └── commands/
│       └── run.rs      # Main execution (incl. inode monitoring)
└── utils/
    └── path.rs         # Path handling
```
