# Running as a System Service

authsock-filter can run as a background service using launchd (macOS) or systemd (Linux).

## Quick Start

```bash
# Register and start the service
authsock-filter service register

# Check status
authsock-filter service status

# Unregister
authsock-filter service unregister
```

## Important: Binary Path Considerations

### When using version managers (mise, aqua, etc.)

If you run `service register` from a version-managed path (mise, aqua, etc.), the command automatically detects this and shows available alternatives:

```
$ authsock-filter service register
ERROR Executable is under mise version manager: ~/.local/share/mise/installs/authsock-filter/0.1.37/authsock-filter

Candidates:
  1. ~/.local/share/mise/installs/authsock-filter/0.1.37/authsock-filter [current, versioned-path]
  2. /opt/homebrew/bin/authsock-filter [symlink:/opt/homebrew/Cellar/authsock-filter/0.1.38/bin/authsock-filter, different-binary]
  3. ~/.local/share/mise/shims/authsock-filter [shim:~/.local/share/mise/installs/authsock-filter/0.1.37/authsock-filter, same-target]

Recommended:
  authsock-filter service register --executable ~/.local/share/mise/shims/authsock-filter

Or force with current path:
  authsock-filter service register --force
```

**Why this matters:** The path contains a version number (`0.1.37`). After upgrading to 0.1.38, this path will no longer exist and the service will fail to start.

**Tags explained:**
- `current`: The executable you're running
- `shim`: A version manager shim that resolves to the current binary
- `same-target`: Points to the same binary as current
- `versioned-path`: Path contains version number (may break on upgrade)
- `different-binary`: A different version of the binary

### What to do

Simply copy and run the **Recommended** command shown in the output. The shim path remains stable across version updates.

If you understand the risk and want to proceed with the versioned path anyway, use `--force`.

### Verifying the registered binary path

```bash
authsock-filter service status
```

Example output:

```
Status: Running (pid: 12345)
Runs: 1 (last exit: (never exited))
KeepAlive: yes

# Command:
/opt/homebrew/bin/authsock-filter run --config ~/.config/authsock-filter/config.toml

# Filter config (~/.config/authsock-filter/config.toml):
/opt/homebrew/bin/authsock-filter run \
  --upstream ~/.ssh/agent.sock \
    --socket ~/.ssh/work.sock 'comment=*@work*' \
    --socket ~/.ssh/personal.sock 'comment=*@personal*'
```

This shows the registered command path, running status, and current configuration.

## Service Commands

| Command | Description |
|---------|-------------|
| `service register` | Create and load the service |
| `service unregister` | Stop and remove the service |
| `service reload` | Restart the service (re-reads config) |
| `service status` | Show service status |

## Configuration

The service reads configuration from the default config file location:
- `~/.config/authsock-filter/config.toml`

To use a different config file, edit the service definition directly or use environment variables.

## Logs

authsock-filter outputs logs to stdout/stderr.

### macOS

launchd captures stdout/stderr but doesn't make them easily accessible. For debugging, run in foreground:

```bash
authsock-filter service unregister
authsock-filter run --config ~/.config/authsock-filter/config.toml
```

### Linux

systemd captures stdout/stderr to the journal:

```bash
# Follow logs
journalctl --user -u authsock-filter -f

# Show recent logs
journalctl --user -u authsock-filter --since "1 hour ago"
```

**Note:** To start user services before login (e.g., for SSH access), enable lingering:

```bash
loginctl enable-linger $USER
```

## SSH Config Integration

You can configure SSH to automatically use a filtered socket based on the repository you're working in.

### Example: Switch IdentityAgent by repository

Add to your `~/.ssh/config`:

```
# Use filtered socket for octo-org repositories
Match exec "git remote get-url origin 2>/dev/null | grep -q 'github.com[/:]octo-org/'"
  IdentityAgent ~/.ssh/work.sock
  ControlPath ~/.ssh/mux-work-%C
```

This configuration:
- Detects the current git repository's remote URL
- Uses `~/.ssh/work.sock` (filtered by authsock-filter) for matching repositories
- Keeps SSH multiplexing sessions separate per organization

**Note:** Replace `octo-org` with your organization name and ensure the socket path matches your authsock-filter configuration.
