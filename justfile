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

# Release with version bump (major, minor, or patch)
release bump="patch":
    #!/usr/bin/env bash
    set -euo pipefail

    # 0. Ensure clean workspace
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "Error: Working tree is not clean" >&2
        git status --short >&2
        exit 1
    fi

    # Pre-checks: fmt, lint, build, test
    cargo fmt --check || { echo "Error: Run 'cargo fmt' first." >&2; exit 1; }
    cargo clippy -- -D warnings
    cargo build --release
    cargo test

    # 1. Version bump in Cargo.toml
    current=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patchv <<< "$current"
    case "{{bump}}" in
        major) major=$((major + 1)); minor=0; patchv=0 ;;
        minor) minor=$((minor + 1)); patchv=0 ;;
        patch) patchv=$((patchv + 1)) ;;
        *) echo "Error: Invalid bump type '{{bump}}' (expected: major, minor, patch)" >&2; exit 1 ;;
    esac
    new_version="${major}.${minor}.${patchv}"
    sed -i '' "s/^version = \"${current}\"/version = \"${new_version}\"/" Cargo.toml
    cargo check --quiet  # Update Cargo.lock
    echo "Version: ${current} -> ${new_version}"

    # 2. CHANGELOG.md update via Claude
    claude "リリースする。 0. ワークスペースがクリーンなことを確認。1.Cargo.tomlのversion bump, 2. CHANGELOG.md更新、3.タグ付け push push --tags, 4. github actionsで自動リリース、5. gh コマンドでworkflowをwatch って流れで2の作業をお願いします。バージョンは v${current} -> v${new_version} です。"

    # Verify CHANGELOG was updated
    if ! git diff --name-only | grep -q CHANGELOG.md; then
        echo "Error: CHANGELOG.md was not updated. Aborting." >&2
        git checkout -- Cargo.toml Cargo.lock
        exit 1
    fi

    # 3. Commit, tag, push
    git add Cargo.toml Cargo.lock CHANGELOG.md
    git diff --cached --stat
    echo ""
    read -rp "Commit and push v${new_version}? [Y/n] " confirm
    case "${confirm:-Y}" in
        [Yy]*|"") ;;
        *)
            echo "Aborted. Restoring changes."
            git checkout -- Cargo.toml Cargo.lock CHANGELOG.md
            exit 1
            ;;
    esac
    git commit -m "Release v${new_version}"
    git tag "v${new_version}"
    git push
    git push --tags

    # 5. Watch GitHub Actions workflow
    gh run watch
