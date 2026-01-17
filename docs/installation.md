# Installation

## Package Managers

### Homebrew (macOS/Linux)

```bash
brew install kawaz/tap/authsock-filter
```

### mise

```bash
mise use github:kawaz/authsock-filter
```

Or add to your `.mise.toml`:

```toml
[tools]
"github:kawaz/authsock-filter" = "latest"
```

### aqua

```bash
aqua g -i kawaz/authsock-filter
```

Or add to your `aqua.yaml`:

```yaml
packages:
  - name: kawaz/authsock-filter@v0.1.37
```

### Cargo (from crates.io)

```bash
cargo install authsock-filter
```

### Cargo (from source)

```bash
cargo install --git https://github.com/kawaz/authsock-filter
```

## Manual Installation

### From GitHub Releases

Download the latest binary from [Releases](https://github.com/kawaz/authsock-filter/releases).

Available binaries:
- `authsock-filter-aarch64-apple-darwin.tar.gz` (macOS Apple Silicon)
- `authsock-filter-x86_64-apple-darwin.tar.gz` (macOS Intel)
- `authsock-filter-x86_64-unknown-linux-gnu.tar.gz` (Linux x86_64)

```bash
# Example: macOS Apple Silicon
curl -sL https://github.com/kawaz/authsock-filter/releases/latest/download/authsock-filter-aarch64-apple-darwin.tar.gz | tar xz
sudo mv authsock-filter /usr/local/bin/
```

### Build from Source

```bash
git clone https://github.com/kawaz/authsock-filter
cd authsock-filter
cargo build --release
sudo cp target/release/authsock-filter /usr/local/bin/
```

## Platform Support

| Platform | Architecture | Status |
|----------|-------------|--------|
| macOS | arm64 (Apple Silicon) | ✓ |
| macOS | x86_64 (Intel) | ✓ |
| Linux | x86_64 | ✓ |
| Linux | arm64 | Planned |
| Windows | - | Not supported (Unix sockets required) |

## Nix (Community)

Nix support is not officially provided, but you can use the following flake:

```nix
{
  inputs.authsock-filter.url = "github:kawaz/authsock-filter";
  # ...
}
```

Or with `nix run`:

```bash
nix run github:kawaz/authsock-filter
```

> Note: Nix flake is not yet available. Contributions welcome!

## Verifying Installation

```bash
authsock-filter --version
```

## Updating

### Homebrew

```bash
brew upgrade authsock-filter
```

### mise

```bash
mise upgrade github:kawaz/authsock-filter
```

### aqua

```bash
aqua update
```

### Cargo

```bash
cargo install authsock-filter
```
