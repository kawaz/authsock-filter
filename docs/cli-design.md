# CLI Design

## Current Design (v0.1.x)

### Multiple Upstream Groups (Implemented)

Each `--upstream` starts a new group. Subsequent `--socket` definitions belong to that group.

```bash
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
    --socket /tmp/mac-personal.sock 'comment=*personal*' \
    --socket /tmp/mac-work.sock 'comment=*work*' \
  --upstream "$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock" \
    --socket /tmp/op-personal.sock 'comment=*personal*' \
    --socket /tmp/op-work.sock 'comment=*work*'
```

### CLI/Config Conversion (Implemented)

- `--print-config`: Export CLI options as TOML config
- `config command`: Generate CLI command from config file

## Future Design

### Feature Flags (Future)

Control SSH agent protocol features per upstream or socket.

#### Upstream-level Options

```bash
--upstream "$SSH_AUTH_SOCK" \
  --allow-add true \
  --allow-remove true \
  --allow-lock true \
  --socket ...

--upstream "~/1password/agent.sock" \
  --allow-add false \      # 1Password doesn't support key addition
  --allow-remove false \   # 1Password doesn't support key removal
  --socket ...
```

#### Socket-level Override

```bash
--socket /tmp/readonly.sock \
  --allow-sign false \     # List keys only, no signing
  comment="*readonly*"
```

### SSH Agent Protocol Messages

| Message | Code | Default | Description |
|---------|------|---------|-------------|
| REQUEST_IDENTITIES | 11 | allow | List keys (filtered) |
| SIGN_REQUEST | 13 | allow | Sign data (filtered) |
| ADD_IDENTITY | 17 | allow | Add key |
| ADD_ID_CONSTRAINED | 25 | allow | Add key with constraints |
| REMOVE_IDENTITY | 18 | allow | Remove key |
| REMOVE_ALL_IDENTITIES | 19 | allow | Remove all keys |
| LOCK | 22 | allow | Lock agent |
| UNLOCK | 23 | allow | Unlock agent |
| EXTENSION | 27 | allow | Protocol extensions |

### Data Structure

```rust
struct UpstreamGroup {
    path: PathBuf,
    options: UpstreamOptions,
    sockets: Vec<SocketSpec>,
}

struct UpstreamOptions {
    allow_add: bool,
    allow_remove: bool,
    allow_lock: bool,
    // ...
}

struct SocketSpec {
    path: PathBuf,
    mode: u32,              // Default: 0o600
    filters: Vec<Filter>,
    options: Option<SocketOptions>,  // Override upstream options
}

struct SocketOptions {
    allow_sign: Option<bool>,   // None = inherit from upstream
    // ...
}
```

### Config File Format (Future)

```toml
[[upstream]]
path = "$SSH_AUTH_SOCK"
allow_add = true
allow_remove = true

[[upstream.socket]]
path = "/tmp/mac-personal.sock"
filters = ["comment=*personal*"]

[[upstream.socket]]
path = "/tmp/mac-work.sock"
filters = ["comment=*work*"]

[[upstream]]
path = "~/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"
allow_add = false
allow_remove = false

[[upstream.socket]]
path = "/tmp/op-personal.sock"
filters = ["comment=*personal*"]
```

## CLI / Config Conversion

### CLI to Config (`--dump-config`)

Export current CLI options as a config file:

```bash
authsock-filter run \
  --upstream "$SSH_AUTH_SOCK" \
  --socket /tmp/work.sock comment=*@work* \
  --socket /tmp/github.sock github=kawaz \
  --dump-config

# Output:
# [[upstream]]
# path = "/private/tmp/com.apple.launchd.xxx/Listeners"
#
# [[upstream.socket]]
# path = "/tmp/work.sock"
# filters = ["comment=*@work*"]
#
# [[upstream.socket]]
# path = "/tmp/github.sock"
# filters = ["github=kawaz"]
```

### Config to CLI (`config --as-cli`)

Generate CLI command from config file:

```bash
authsock-filter config --as-cli

# Output:
# authsock-filter run \
#   --upstream '/private/tmp/com.apple.launchd.xxx/Listeners' \
#   --socket '/tmp/work.sock' 'comment=*@work*' \
#   --socket '/tmp/github.sock' 'github=kawaz'
```

Use cases:
- Save trial-and-error CLI experiments as config
- Reproduce config behavior with one-liner
- CI/script integration
- Config debugging and sharing

## Implementation Phases

1. **Phase 1**: Multiple upstream groups with `--upstream` as delimiter
2. **Phase 2**: Feature flags per upstream (`--allow-add`, etc.)
3. **Phase 3**: Socket-level option overrides
4. **Phase 4**: Config file format update
5. **Phase 5**: CLI/Config bidirectional conversion
