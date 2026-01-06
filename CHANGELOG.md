# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
- README.md: Fixed filter syntax in examples (`type:value` → `type=value`)
- README.md: Added shell quoting to all glob pattern examples
- README.md: Updated command structure to match implementation
- docs/cli-design.md: Updated to reflect current implementation status

## [0.1.32] - 2026-01-06

### Added
- Exit on socket inode change detection (security improvement)
- Comprehensive review prompts document

### Fixed
- Actually use run.rs for inode monitoring instead of server.rs

## [0.1.31] - 2026-01-06

### Added
- Socket file replacement/removal detection
- Auto-exit when upstream socket is modified

## [0.1.30] - 2026-01-05

### Added
- Merge same-path `--socket` arguments as OR filter groups
- Compound filter syntax: `["f1", "f2", ["f3", "f4"]]` for `f1 || f2 || (f3 && f4)`

## [0.1.21] - 2026-01-04

### Added
- Service status command
- Lefthook for pre-push checks

### Changed
- Extract LABEL_PREFIX constant for launchd service labels
- Improve service register candidate display with shim/symlink resolution

### Fixed
- Clippy warnings (collapsible_if)
- Service register suggestion to use argv[0]

## [0.1.15] - 2026-01-03

### Added
- build.rs to display rustc version in version command
- Config file support with per-socket upstream

### Changed
- Negation filter prefix from `-` to `not-`
- Rename `config --show-default` to `--example`

## [0.1.14] - 2026-01-02

### Added
- Project rules for pre-commit checks

### Fixed
- Clippy for-kv-map warning

## [0.1.13] - 2026-01-01

### Changed
- Refactor: simplify codebase by removing unused features
