# authsock-filter

SSH agent proxy with filtering and logging. Create multiple filtered sockets from a single upstream SSH agent.

## Features

- **Multiple filtered sockets**: Create separate agent sockets with different key visibility
- **Flexible filtering**: Filter keys by fingerprint, comment, key type, GitHub user keys, or keyfile
- **Pattern matching**: Support for exact match, glob patterns, and regular expressions
- **Negation support**: Exclude keys with `-` prefix
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
authsock-filter run -s /tmp/work.sock:comment:*@work*

# Use the filtered socket
SSH_AUTH_SOCK=/tmp/work.sock ssh user@work-server
```

## Usage

```bash
authsock-filter [OPTIONS] <COMMAND>

Commands:
  run         Run the proxy in foreground
  start       Start the proxy as a daemon
  stop        Stop the running daemon
  status      Show daemon status
  config      Show configuration
  version     Show version information
  upgrade     Upgrade to the latest version
  register    Register as OS service (launchd/systemd)
  unregister  Unregister OS service
  completion  Generate shell completion scripts

Options:
  -c, --config <PATH>  Configuration file path
  -v, --verbose        Increase verbosity
  -q, --quiet          Decrease verbosity
  -h, --help           Print help
```

### Run Command Options

```bash
authsock-filter run [OPTIONS]

Options:
  -u, --upstream <SOCKET>  Upstream agent socket [default: $SSH_AUTH_SOCK]
      --log <PATH>         JSONL log output path
  -s, --socket <SPEC>      Socket definition (repeatable)
```

### Socket Definition Format

```
/path/to/socket.sock:filter1:filter2:...
```

Multiple filters on the same socket are ANDed together.

## Filter Types

| Type | Syntax | Example |
|------|--------|---------|
| Fingerprint | `fingerprint:SHA256:xxx` | `fingerprint:SHA256:abc123...` |
| Fingerprint (auto) | `SHA256:xxx` | `SHA256:abc123...` |
| Public key | `pubkey:ssh-ed25519 AAAA...` | Full public key (comment ignored) |
| Public key (auto) | `ssh-ed25519 AAAA...` | Auto-detected by key type prefix |
| Keyfile | `keyfile:~/.ssh/allowed_keys` | Keys from authorized_keys format file |
| Comment (exact) | `comment:user@host` | Exact match |
| Comment (glob) | `comment:*@work*` | Glob pattern |
| Comment (regex) | `comment:~@work\.example\.com$` | Regex with `~` prefix |
| Key type | `type:ed25519` | `ed25519`, `rsa`, `ecdsa`, `dsa` |
| GitHub user | `github:username` | Keys from github.com/username.keys |
| Negation | `-<filter>` | `-type:dsa` (exclude DSA keys) |

## Configuration File

Create `~/.config/authsock-filter/config.toml`:

```toml
# Global settings
upstream = "$SSH_AUTH_SOCK"
log_path = "$XDG_STATE_HOME/authsock-filter/messages.jsonl"

# Socket definitions
[sockets.work]
path = "$XDG_RUNTIME_DIR/authsock-filter/work.sock"
filters = ["comment:~@work\\.example\\.com$"]

[sockets.personal]
path = "~/.ssh/personal-agent.sock"
filters = [
    "github:kawaz",
    "type:ed25519",
]

[sockets.no-dsa]
path = "$XDG_RUNTIME_DIR/authsock-filter/no-dsa.sock"
filters = ["-type:dsa"]

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
  -s ~/.ssh/work.sock:comment:*@work.example.com \
  -s ~/.ssh/personal.sock:-comment:*@work.example.com
```

### Only Modern Keys

```bash
# Only allow ed25519 keys
authsock-filter run -s /tmp/modern.sock:type:ed25519:-type:dsa:-type:rsa
```

### GitHub Authorized Keys

```bash
# Only allow keys registered with your GitHub account
authsock-filter run -s /tmp/github.sock:github:kawaz
```

### Combining Filters

```bash
# Work keys that are also ed25519
authsock-filter run \
  -s /tmp/work-ed25519.sock:comment:*@work*:type:ed25519
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
authsock-filter register

# Unregister
authsock-filter unregister
```

### Linux (systemd)

```bash
# Register as systemd user service
authsock-filter register

# Unregister
authsock-filter unregister
```

## Signal Handling

- `SIGTERM`, `SIGINT`: Graceful shutdown (cleanup sockets)
- `SIGHUP`: Reload configuration and refresh GitHub/keyfile caches

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

## TODO

- [ ] Register to mise registry for `mise use authsock-filter` support

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

Yoshiaki Kawazu ([@kawaz](https://github.com/kawaz))
