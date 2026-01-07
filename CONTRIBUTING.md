# Contributing to authsock-filter

## Development Setup

```bash
# Clone the repository
git clone https://github.com/kawaz/authsock-filter.git
cd authsock-filter

# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```

## Release Process

Releases are automated via GitHub Actions. To create a new release:

### 1. Update version

Edit `Cargo.toml` and update the version number:

```toml
[package]
version = "X.Y.Z"
```

### 2. Commit and push

```bash
git add Cargo.toml Cargo.lock
git commit -m "Bump version to X.Y.Z"
git push
```

### 3. Create and push tag

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

### 4. CI handles the rest

The CI workflow will:
1. Run tests on all platforms (ubuntu, macos) with stable and beta Rust
2. Build release binaries for all targets:
   - `x86_64-unknown-linux-gnu`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
3. Create a GitHub Release with all assets attached

**Important**: Do NOT manually create the release via `gh release create`. The CI will create it automatically after all assets are built. This ensures users won't encounter missing assets when upgrading.

## Code Style

- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes without warnings
- Write tests for new functionality
- Keep commits focused and atomic

## Pull Requests

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Ensure tests pass
5. Submit a pull request

## Reporting Issues

Please open an issue on GitHub with:
- A clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- Your environment (OS, Rust version, etc.)
