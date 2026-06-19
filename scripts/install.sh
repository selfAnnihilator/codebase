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

path_contains_dir() {
    dir=$1
    case ":$PATH:" in
        *":$dir:"*) return 0 ;;
        *) return 1 ;;
    esac
}

bin_dir_writable_or_creatable() {
    dir=$1
    if [ -d "$dir" ]; then
        [ -w "$dir" ]
        return
    fi

    parent=$(dirname -- "$dir")
    [ -d "$parent" ] && [ -w "$parent" ]
}

choose_install_bin_dir() {
    if [ "${INSTALL_BIN_DIR:-}" ]; then
        printf '%s\n' "$INSTALL_BIN_DIR"
        return
    fi

    if path_contains_dir "$HOME/.local/bin" && bin_dir_writable_or_creatable "$HOME/.local/bin"; then
        printf '%s\n' "$HOME/.local/bin"
        return
    fi

    if path_contains_dir "/usr/local/bin" && bin_dir_writable_or_creatable "/usr/local/bin"; then
        printf '%s\n' "/usr/local/bin"
        return
    fi

    old_ifs=$IFS
    IFS=:
    for dir in $PATH; do
        [ -n "$dir" ] || continue
        case "$dir" in
            */bin)
                if bin_dir_writable_or_creatable "$dir"; then
                    printf '%s\n' "$dir"
                    IFS=$old_ifs
                    return
                fi
                ;;
        esac
    done
    IFS=$old_ifs

    cat >&2 <<'EOF'
error: could not find a writable bin directory on PATH.

Try one of these:
  1. Create a user bin directory that is on PATH, such as ~/.local/bin
  2. Re-run with a writable override:
     INSTALL_BIN_DIR="$HOME/.local/bin" ./scripts/install.sh
  3. Re-run for a system-wide install:
     sudo INSTALL_BIN_DIR=/usr/local/bin ./scripts/install.sh
EOF
    exit 1
}

if ! command -v cargo >/dev/null 2>&1; then
    print_cargo_help
    exit 1
fi

install_bin_dir=$(choose_install_bin_dir)
install_root=$(dirname -- "$install_bin_dir")

case "$install_bin_dir" in
    */bin) ;;
    *)
        echo "error: install bin directory must end with /bin: $install_bin_dir" >&2
        exit 1
        ;;
esac

mkdir -p "$install_bin_dir"

echo "Installing Code Base from $repo_root"
echo "Install location: $install_bin_dir"
cargo install --path "$repo_root" --force --locked --root "$install_root"

cb_path="$install_bin_dir/cb"
cb_tui_path="$install_bin_dir/cb-tui"

if [ ! -x "$cb_path" ] || [ ! -x "$cb_tui_path" ]; then
    echo "error: install completed, but expected binaries were not found in $install_bin_dir" >&2
    exit 1
fi

"$cb_path" --help >/dev/null
"$cb_tui_path" --help >/dev/null

echo "Installed:"
echo "  cb: $cb_path"
echo "  cb-tui: $cb_tui_path"

if ! path_contains_dir "$install_bin_dir"; then
    echo
    echo "warning: install directory is not on PATH for this shell."
    echo "Add it with:"
    echo "  export PATH=\"$install_bin_dir:\$PATH\""
fi
