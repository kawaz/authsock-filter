# default: fmt, lint, build, test
default: fmt lint build test

bin := "target/release/authsock-filter"

run *args: ensure-build
    {{bin}} {{args}}

ensure-build:
    #!/usr/bin/env bash
    if [[ ! -x {{bin}} ]] || [[ -n $(find src -name '*.rs' -newer {{bin}}) ]]; then
        cargo build --release
    fi

fmt:
    cargo fmt

lint:
    cargo clippy -- -D warnings

build:
    cargo build --release

test:
    cargo test
