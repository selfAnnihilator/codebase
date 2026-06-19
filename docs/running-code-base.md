# Running Code Base

Code Base is a Rust CLI/TUI app with two binaries:

- `cb`: main CLI command.
- `cb-tui`: direct launcher for the TUI.

## Prerequisites

- Rust and Cargo.
- A terminal for the TUI.
- `nvim` on `PATH` if you use the default editor. You can configure another editor.

## Install Locally

The install scripts are distro-neutral POSIX shell scripts. They install from this checkout using Cargo into a real `PATH` bin directory, so `cb` and `cb-tui` are ready to use immediately on normal Linux setups.

Install Rust and Cargo with rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Or use your distro package manager:

```bash
# Debian/Ubuntu
sudo apt install cargo

# Fedora/RHEL
sudo dnf install cargo

# Arch
sudo pacman -S rust

# openSUSE
sudo zypper install cargo

# Alpine
sudo apk add cargo rust

# Void
sudo xbps-install rust cargo
```

Install both binaries:

```bash
./scripts/install.sh
```

By default the script installs into a writable `bin` directory that is already on `PATH`, preferring:

```text
~/.local/bin
/usr/local/bin
```

This provides:

```text
cb
cb-tui
```

For an explicit target directory:

```bash
INSTALL_BIN_DIR="$HOME/.local/bin" ./scripts/install.sh
```

For a system-wide install:

```bash
sudo INSTALL_BIN_DIR=/usr/local/bin ./scripts/install.sh
```

Uninstall the binaries:

```bash
./scripts/uninstall.sh
```

Uninstall does not remove your registry data or config.

Manual Cargo install is also supported:

```bash
cargo install --path .
```

After install:

```bash
cb --help
cb-tui --help
```

## Build

Build commands are only needed for development. Normal usage after install uses `cb` and `cb-tui`.

```bash
cargo build
```

Build release binaries:

```bash
cargo build --release
```

Debug binaries are written to:

```text
target/debug/cb
target/debug/cb-tui
```

Release binaries are written to:

```text
target/release/cb
target/release/cb-tui
```

## Run The CLI

Show help:

```bash
cb --help
```

Register the current directory:

```bash
cb init
```

Register a specific project without prompts:

```bash
cb init ~/work/my-project --name "My Project" --tag work --no-prompt
```

List projects:

```bash
cb list
```

Search projects:

```bash
cb search api
```

Open a project:

```bash
cb open api
```

View project docs:

```bash
cb doc api
```

## Run The TUI

Run through the main CLI:

```bash
cb tui
```

Or run the dedicated TUI binary:

```bash
cb-tui
```

Inside the TUI:

- Type to search.
- `Enter` opens the selected project.
- `1` selects Docs.
- `2` selects Tree.
- `Tab` switches focus.
- `Ctrl+r` cycles sort mode.
- `Ctrl+o` toggles sort order.
- `d` opens or creates the selected project docs.
- `e` edits selected project metadata.
- `q` exits.

## Use An Isolated Registry

For testing without touching your real registry/config:

```bash
tmp="$(mktemp -d)"
mkdir -p "$tmp/data" "$tmp/config" "$tmp/project"
printf '# Demo\n' > "$tmp/project/README.md"

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb init "$tmp/project" --name Demo --no-prompt

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb list

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cb-tui
```

## Configure The Editor

Default editor is `nvim`.

Set a different simple editor:

```bash
cb config set editor code
```

Set a custom command template:

```bash
cb config set editor_command 'tmux new-window -c {path} nvim .'
```

For project opening, `{path}` is the project directory. For docs editing, `{path}` is the docs file.

## Verify The Project

Run formatting, tests, and lint checks:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
