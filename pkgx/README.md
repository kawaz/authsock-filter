# pkgx Registry Support

This directory contains the package definition for [pkgx](https://pkgx.sh/).

## File

- `package.yml` - Package definition for pkgx pantry

## Submitting to pkgx pantry

To add authsock-filter to the pkgx pantry:

1. Fork [pkgxdev/pantry](https://github.com/pkgxdev/pantry)
2. Create a new directory: `projects/github.com/kawaz/authsock-filter/`
3. Copy `package.yml` to that directory
4. Submit a pull request

## Testing locally

```bash
# Install pkgx
curl -Ssf https://pkgx.sh | sh

# Build and test the package locally (requires brewkit)
pkgx +brewkit bk build github.com/kawaz/authsock-filter
pkgx +brewkit bk test github.com/kawaz/authsock-filter
```

## References

- [pkgx documentation](https://docs.pkgx.sh/)
- [pkgx pantry](https://github.com/pkgxdev/pantry)
- [Contributing to pantry](https://docs.pkgx.sh/appendix/packaging/pantry)
