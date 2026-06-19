# Release Checklist

Use this checklist before tagging a release.

## Preconditions

- Working tree is clean.
- `Cargo.toml` version is the intended release version.
- `Cargo.lock` is committed.
- Release licensing is intentional. Current package metadata is `UNLICENSED`; choose and add a license before public redistribution.
- User-facing docs match current behavior:
  - `README.md`
  - `docs/running-code-base.md`
  - `docs/implementation-status.md`

## Verification

Run locally:

```bash
sh -n scripts/install.sh
sh -n scripts/uninstall.sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Run an isolated install smoke:

```bash
tmp="$(mktemp -d)"
mkdir -p "$tmp/bin"
PATH="$tmp/bin:$PATH" INSTALL_BIN_DIR="$tmp/bin" ./scripts/install.sh
"$tmp/bin/cb" --help >/dev/null
"$tmp/bin/cb-tui" --help >/dev/null
INSTALL_BIN_DIR="$tmp/bin" ./scripts/uninstall.sh
```

## Manual Smoke

Use an isolated registry/config:

```bash
tmp="$(mktemp -d)"
mkdir -p "$tmp/data" "$tmp/config" "$tmp/project"
printf '# Demo\n' > "$tmp/project/README.md"

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb init "$tmp/project" --name Demo --tag demo --no-prompt

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb list

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb search demo

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb-tui
```

## Tagging

```bash
git tag v0.1.0
git push --tags
```

Use the version from `Cargo.toml`.
