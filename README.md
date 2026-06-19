# Code Base

Code Base is a local project registry and launcher. It stores metadata about existing project directories, lets you search by name/path/tags, previews docs and file trees in a terminal UI, and opens projects or docs in your configured editor.

Repository: <https://github.com/selfAnnihilator/codebase>

It installs two commands:

- `cb`: main CLI.
- `cb-tui`: direct terminal UI launcher.

## Install

Prerequisite: Rust/Cargo. The distro-neutral route is rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install from this checkout:

```bash
./scripts/install.sh
```

The script installs into a real `PATH` bin directory, preferring `~/.local/bin` and then `/usr/local/bin`.

For a system-wide install:

```bash
sudo INSTALL_BIN_DIR=/usr/local/bin ./scripts/install.sh
```

Uninstall:

```bash
./scripts/uninstall.sh
```

The uninstall script removes the binaries but leaves your registry and config data in place.

## Quick Start

Register the current directory:

```bash
cb init
```

Register a project non-interactively:

```bash
cb init ~/work/api --name "API" --tag work --no-prompt
```

Search and open projects:

```bash
cb search api
cb open api
```

Launch the TUI:

```bash
cb tui
# or
cb-tui
```

Inside the TUI:

- `/` enters search mode.
- `q` exits.
- `Enter` opens the selected project.
- `d` opens or creates project docs.
- `Delete` removes the selected registry entry after confirmation.
- `Shift+Delete` permanently deletes the selected project after confirmation.

## Development

Run the normal verification set:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

See [docs/running-code-base.md](docs/running-code-base.md) for more usage details and [docs/code-base-v1-spec.md](docs/code-base-v1-spec.md) for the v1 product spec.
