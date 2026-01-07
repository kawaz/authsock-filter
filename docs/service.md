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

If you installed authsock-filter via mise, aqua, or similar version managers, the binary path changes with each version update. This can cause the service to fail after upgrades.

**Problem:**
```bash
# mise installs to version-specific paths like:
~/.local/share/mise/installs/authsock-filter/0.1.37/authsock-filter

# After upgrade to 0.1.38, the old path no longer exists
```

**Solutions:**

#### Option 1: Use a stable symlink (Recommended)

Create a stable path that points to the current version:

```bash
# Create stable bin directory
mkdir -p ~/.local/bin

# Create symlink (mise automatically manages this if ~/.local/bin is in PATH)
ln -sf "$(mise which authsock-filter)" ~/.local/bin/authsock-filter

# Register service with stable path
authsock-filter service register
```

mise automatically updates `~/.local/bin` symlinks when you run `mise use`.

#### Option 2: Use Homebrew instead

Homebrew installs to a stable path (`/opt/homebrew/bin/authsock-filter` on Apple Silicon):

```bash
brew install kawaz/tap/authsock-filter
authsock-filter service register
```

#### Option 3: Copy binary to stable location

```bash
sudo cp "$(which authsock-filter)" /usr/local/bin/authsock-filter
/usr/local/bin/authsock-filter service register
```

#### Option 4: Re-register after each upgrade

```bash
mise upgrade authsock-filter
authsock-filter service unregister
authsock-filter service register
```

### Verifying the registered binary path

**macOS (launchd):**
```bash
# Check the plist file
cat ~/Library/LaunchAgents/com.github.kawaz.authsock-filter.plist | grep ProgramArguments -A2
```

**Linux (systemd):**
```bash
# Check the service file
cat ~/.config/systemd/user/authsock-filter.service | grep ExecStart
```

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

### macOS

```bash
# Stream logs
log stream --predicate 'subsystem == "com.github.kawaz.authsock-filter"' --level info

# Show recent logs
log show --predicate 'subsystem == "com.github.kawaz.authsock-filter"' --last 1h
```

Or check the log file (if configured):
```bash
tail -f ~/.local/state/authsock-filter/authsock-filter.log
```

### Linux

```bash
# Follow logs
journalctl --user -u authsock-filter -f

# Show recent logs
journalctl --user -u authsock-filter --since "1 hour ago"
```

## Troubleshooting

### Service fails to start

1. **Check if binary exists at registered path:**
   ```bash
   # macOS
   plutil -p ~/Library/LaunchAgents/com.github.kawaz.authsock-filter.plist

   # Linux
   systemctl --user cat authsock-filter
   ```

2. **Check if config file is valid:**
   ```bash
   authsock-filter config show
   ```

3. **Check logs for errors** (see Logs section above)

### Service doesn't start on login

**macOS:**
```bash
# Ensure the plist is loaded
launchctl list | grep authsock-filter

# If not listed, load it manually
launchctl load ~/Library/LaunchAgents/com.github.kawaz.authsock-filter.plist
```

**Linux:**
```bash
# Enable the service
systemctl --user enable authsock-filter

# Check if user lingering is enabled (for services to start without login)
loginctl show-user $USER | grep Linger
```

### After upgrade, service uses old binary

Re-register the service:
```bash
authsock-filter service unregister
authsock-filter service register
```

Or if using mise, ensure your shell is reloaded to pick up the new path:
```bash
exec $SHELL
authsock-filter service reload
```
