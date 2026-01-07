# authsock-filter

SSH agent proxy with filtering and logging. Create multiple filtered sockets from a single upstream SSH agent.

## Motivation

SSH agents present **all registered keys** to any server you connect to. This means:

- Remote servers can see fingerprints of keys you don't intend to use
- Unintended key exposure may leak information about your identity or organization
- You have no control over which keys are offered during authentication

**authsock-filter** solves this by creating filtered proxy sockets that only expose the keys you explicitly allow. Connect to work servers with only work keys, personal servers with only personal keys.

## Features

- **Multiple filtered sockets**: Create separate agent sockets with different key visibility
- **Flexible filtering**: Filter keys by fingerprint, comment, key type, GitHub user keys, or keyfile
- **Pattern matching**: Support for exact match, glob patterns, and regular expressions
- **Negation support**: Exclude keys with `not-` prefix
- **JSONL logging**: Log all agent operations for auditing
- **Daemon mode**: Run as a background service
- **OS integration**: Register as launchd (macOS) or systemd (Linux) service

## Installation

### From source

```bash
cargo install --git https://github.com/kawaz/authsock-filter
```

### From releases

Download the latest binary from [Releases](https://github.com/kawaz/authsock-filter/releases).

## Quick Start

```bash
# Create a filtered socket that only shows keys with "@work" in the comment
authsock-filter run --upstream "$SSH_AUTH_SOCK" --socket /tmp/work.sock 'comment=*@work*'

# Use the filtered socket
SSH_AUTH_SOCK=/tmp/work.sock ssh user@work-server
```

## Verification

Compare the keys before and after filtering:

```bash
# List all keys in your original agent
ssh-add -l

# List keys visible through the filtered socket
SSH_AUTH_SOCK=/tmp/work.sock ssh-add -l
```

The filtered socket should only show keys matching your filter criteria.

## Usage

```bash
authsock-filter [OPTIONS] [COMMAND]

Commands:
  run         Run the proxy in the foreground
  config      Manage configuration file (show, edit, path, command)
  service     Manage OS service (register, unregister, reload, status)
  completion  Generate shell completions

Options:
      --config <PATH>  Configuration file path [env: AUTHSOCK_FILTER_CONFIG]
  -v, --verbose        Enable verbose output
      --quiet          Suppress non-essential output
  -V, --version        Print version
  -h, --help           Print help
```

### Run Command Options

```bash
authsock-filter run [OPTIONS]

Options:
  --upstream <SOCKET>        Upstream agent socket [default: $SSH_AUTH_SOCK]
  --socket <PATH> [ARGS...]  Socket definition with inline filters
  --print-config             Output equivalent configuration and exit
```

### Upstream Groups

Each `--upstream` starts a new group. Subsequent `--socket` definitions belong to that upstream:

```bash
# Single upstream (default uses $SSH_AUTH_SOCK)
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
  --socket /tmp/work.sock 'comment=*@work*' 'type=ed25519' \
  --socket /tmp/github.sock 'github=kawaz'

# Multiple upstreams (e.g., macOS Keychain + 1Password)
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
    --socket /tmp/mac-work.sock 'comment=*@work*' \
    --socket /tmp/mac-personal.sock 'not-comment=*@work*' \
  --upstream "$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock" \
    --socket /tmp/1p-github.sock 'github=kawaz'
```

### Socket and Filter Format

Arguments after `--socket PATH` until the next `--socket` or `--upstream` are filters:

Filters use `type=value` format. Multiple filters on the same socket are ANDed together.

## Filter Types

| Type | Syntax | Description |
|------|--------|-------------|
| Fingerprint | `fingerprint=SHA256:xxx` | Match by key fingerprint |
| Comment | `comment=pattern` | Match by comment (glob or `~regex`) |
| GitHub | `github=username` | Match keys from github.com/username.keys |
| Key type | `type=ed25519` | Match by type: `ed25519`, `rsa`, `ecdsa`, `dsa` |
| Public key | `pubkey=ssh-ed25519 AAAA...` | Match by full public key |
| Keyfile | `keyfile=~/.ssh/allowed_keys` | Match keys from file |
| Negation | `not-type=value` | Prefix with `not-` to exclude |

## Configuration File

Create `~/.config/authsock-filter/config.toml`:

```toml
# Global settings
upstream = "$SSH_AUTH_SOCK"

# Socket definitions
[sockets.work]
path = "$XDG_RUNTIME_DIR/authsock-filter/work.sock"
filters = ["comment=~@work\\.example\\.com$"]

[sockets.personal]
path = "~/.ssh/personal-agent.sock"
# Multiple filters in inner array = AND, multiple arrays = OR
# e.g., [["f1", "f2"], "f3"] means (f1 AND f2) OR f3
filters = [
    ["github=kawaz", "type=ed25519"],
]

[sockets.no-dsa]
path = "$XDG_RUNTIME_DIR/authsock-filter/no-dsa.sock"
filters = ["not-type=dsa"]

# GitHub cache settings (optional)
[github]
cache_ttl = "1h"
timeout = "10s"
```

## Examples

### Work vs Personal Keys

```bash
# Create separate sockets for work and personal use
authsock-filter run \
  --socket ~/.ssh/work.sock 'comment=*@work.example.com' \
  --socket ~/.ssh/personal.sock 'not-comment=*@work.example.com'
```

### Only Modern Keys

```bash
# Only allow ed25519 keys
authsock-filter run \
  --socket /tmp/modern.sock 'type=ed25519'
```

### GitHub Authorized Keys

```bash
# Only allow keys registered with your GitHub account
authsock-filter run --socket /tmp/github.sock 'github=kawaz'
```

### Combining Filters

```bash
# Work keys that are also ed25519
authsock-filter run --socket /tmp/work-ed25519.sock 'comment=*@work*' 'type=ed25519'
```

## Environment Variables

- `SSH_AUTH_SOCK`: Default upstream agent socket
- `XDG_CONFIG_HOME`: Config file location (default: `~/.config`)
- `XDG_RUNTIME_DIR`: Runtime directory for sockets and PID file
- `XDG_STATE_HOME`: State directory for logs (default: `~/.local/state`)

## OS Service Registration

### macOS (launchd)

```bash
# Register as launchd service
authsock-filter service register

# Check status
authsock-filter service status

# Unregister
authsock-filter service unregister
```

### Linux (systemd)

```bash
# Register as systemd user service
authsock-filter service register

# Check status
authsock-filter service status

# Unregister
authsock-filter service unregister
```

## Signal Handling

- `SIGTERM`, `SIGINT`: Graceful shutdown (cleanup sockets)

## Shell Completion

Add to your shell configuration:

```bash
# Bash (~/.bashrc)
source <(authsock-filter completion bash)

# Zsh (~/.zshrc)
source <(authsock-filter completion zsh)

# Fish (~/.config/fish/config.fish)
authsock-filter completion fish | source
```

## Roadmap

### Implemented
- Dynamic shell completion using `CompleteEnv` (clap_complete unstable-dynamic)
- Custom completion for `--socket` inline filters
- Multiple upstream support (each `--upstream` starts a new group)
- CLI/Config conversion (`--print-config`, `config command`)

### Planned
- Feature flags per upstream (`--allow-add`, `--allow-remove`, etc.)
- Socket-specific options (`--mode`, etc.)
- Register to mise registry

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

Yoshiaki Kawazu ([@kawaz](https://github.com/kawaz))
