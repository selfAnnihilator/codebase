#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

print_cargo_help() {
    cat >&2 <<'EOF'
error: cargo is required to install Code Base.

Install Rust and Cargo with rustup, which works on most Linux distros:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

Or install Cargo with your distro package manager:
  Debian/Ubuntu:  sudo apt install cargo
  Fedora/RHEL:    sudo dnf install cargo
  Arch:           sudo pacman -S rust
  openSUSE:       sudo zypper install cargo
  Alpine:         sudo apk add cargo rust
  Void:           sudo xbps-install rust cargo

After installing Cargo, rerun:
  ./scripts/install.sh
EOF
}

if ! command -v cargo >/dev/null 2>&1; then
    print_cargo_help
    exit 1
fi

cargo_home=${CARGO_HOME:-"$HOME/.cargo"}
cargo_bin="$cargo_home/bin"

echo "Installing Code Base from $repo_root"
cargo install --path "$repo_root" --force --locked

cb_path=
cb_tui_path=

if [ -x "$cargo_bin/cb" ]; then
    cb_path=$cargo_bin/cb
elif command -v cb >/dev/null 2>&1; then
    cb_path=$(command -v cb)
fi

if [ -x "$cargo_bin/cb-tui" ]; then
    cb_tui_path=$cargo_bin/cb-tui
elif command -v cb-tui >/dev/null 2>&1; then
    cb_tui_path=$(command -v cb-tui)
fi

if [ -z "$cb_path" ] || [ -z "$cb_tui_path" ]; then
    echo "warning: install completed, but installed commands were not found on PATH" >&2
    echo "Add Cargo's bin directory to PATH:" >&2
    echo "  export PATH=\"$cargo_bin:\$PATH\"" >&2
    exit 1
fi

"$cb_path" --help >/dev/null
"$cb_tui_path" --help >/dev/null

echo "Installed:"
echo "  cb: $cb_path"
echo "  cb-tui: $cb_tui_path"

case ":$PATH:" in
    *":$cargo_bin:"*) ;;
    *)
        echo
        echo "Cargo's bin directory is not on PATH for this shell."
        echo "Add it with:"
        echo "  export PATH=\"$cargo_bin:\$PATH\""
        ;;
esac
