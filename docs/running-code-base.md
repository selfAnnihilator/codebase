# Running Code Base

Code Base is a Rust CLI/TUI app with two binaries:

- `cb`: main CLI command.
- `cb-tui`: direct launcher for the TUI.

## Prerequisites

- Rust and Cargo.
- A terminal for the TUI.
- `nvim` on `PATH` if you use the default editor. You can configure another editor.

## Build

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

Because the package has two binaries, use `--bin` when running through Cargo:

```bash
cargo run --bin cb -- --help
```

Register the current directory:

```bash
cargo run --bin cb -- init
```

Register a specific project without prompts:

```bash
cargo run --bin cb -- init ~/work/my-project --name "My Project" --tag work --no-prompt
```

List projects:

```bash
cargo run --bin cb -- list
```

Search projects:

```bash
cargo run --bin cb -- search api
```

Open a project:

```bash
cargo run --bin cb -- open api
```

View project docs:

```bash
cargo run --bin cb -- doc api
```

## Run The TUI

Run through the main CLI:

```bash
cargo run --bin cb -- tui
```

Or run the dedicated TUI binary:

```bash
cargo run --bin cb-tui
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
cargo run --bin cb -- init "$tmp/project" --name Demo --no-prompt

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cargo run --bin cb -- list

CODEBASE_DATA_DIR="$tmp/data" \
CODEBASE_CONFIG_DIR="$tmp/config" \
cargo run --bin cb-tui
```

## Configure The Editor

Default editor is `nvim`.

Set a different simple editor:

```bash
cargo run --bin cb -- config set editor code
```

Set a custom command template:

```bash
cargo run --bin cb -- config set editor_command 'tmux new-window -c {path} nvim .'
```

For project opening, `{path}` is the project directory. For docs editing, `{path}` is the docs file.

## Verify The Project

Run formatting, tests, and lint checks:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## Install Locally

The install scripts are distro-neutral POSIX shell scripts. They install from this checkout using Cargo, so they work on any Linux distro with Rust/Cargo installed.

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

This installs the Cargo package from the current checkout and provides:

```text
cb
cb-tui
```

If Cargo's bin directory is not on `PATH`, the script prints the exact `export PATH=...` line to add.

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
