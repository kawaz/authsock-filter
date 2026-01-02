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
authsock-filter run --upstream "$SSH_AUTH_SOCK" --socket /tmp/work.sock comment=*@work*

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
  --config <PATH>  Configuration file path
  --verbose        Increase verbosity
  --quiet          Decrease verbosity
  --help           Print help
```

### Run Command Options

```bash
authsock-filter run [OPTIONS]

Options:
  --upstream <SOCKET>        Upstream agent socket [default: $SSH_AUTH_SOCK]
  --log <PATH>               JSONL log output path
  --socket <PATH> [ARGS...]  Socket definition with inline filters
```

### Upstream Groups

Each `--upstream` starts a new group. Subsequent `--socket` definitions belong to that upstream:

```bash
# Single upstream (default uses $SSH_AUTH_SOCK)
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
  --socket /tmp/work.sock comment=*@work* type=ed25519 \
  --socket /tmp/github.sock github=kawaz

# Multiple upstreams (e.g., macOS Keychain + 1Password)
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
    --socket /tmp/mac-work.sock comment=*@work* \
    --socket /tmp/mac-personal.sock -comment=*@work* \
  --upstream ~/Library/Group\ Containers/2BUA8C4S2C.com.1password/t/agent.sock \
    --socket /tmp/1p-github.sock github=kawaz
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
| Negation | `-type=value` | Prefix with `-` to exclude |

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
  --socket ~/.ssh/work.sock comment=*@work.example.com \
  --socket ~/.ssh/personal.sock -comment=*@work.example.com
```

### Only Modern Keys

```bash
# Only allow ed25519 keys
authsock-filter run \
  --socket /tmp/modern.sock type=ed25519 -type=dsa -type=rsa
```

### GitHub Authorized Keys

```bash
# Only allow keys registered with your GitHub account
authsock-filter run --socket /tmp/github.sock github=kawaz
```

### Combining Filters

```bash
# Work keys that are also ed25519
authsock-filter run --socket /tmp/work-ed25519.sock comment=*@work* type=ed25519
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

- [x] Dynamic shell completion using `CompleteEnv` (clap_complete unstable-dynamic)
  - Binary-as-completion-engine for minimal shell memory footprint
- [x] Custom completion for `--socket` inline filters
  - Filter type completion (comment=, fingerprint=, github=, type=, etc.)
  - Negation filter completion (-type=, -comment=, etc.)
  - Key type value completion for type= and -type=
  - Path completion for socket paths
- [x] Multiple upstream support
  - Each `--upstream` starts a new group
  - Enables using multiple SSH agents (e.g., macOS Keychain + 1Password)
- [ ] Feature flags per upstream (`--allow-add`, `--allow-remove`, etc.)
- [ ] Socket-specific options (`--mode`, etc.)
- [ ] CLI/Config bidirectional conversion (`--dump-config`, `config --as-cli`)
- [ ] Unify config file filter format with CLI (`type=value` style)
- [ ] Register to mise registry for `mise use authsock-filter` support

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

Yoshiaki Kawazu ([@kawaz](https://github.com/kawaz))
