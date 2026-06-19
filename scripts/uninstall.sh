#!/bin/sh
set -eu

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

    if path_contains_dir "$HOME/.local/bin" && [ -e "$HOME/.local/bin/cb" -o -e "$HOME/.local/bin/cb-tui" ]; then
        printf '%s\n' "$HOME/.local/bin"
        return
    fi

    if [ -e "/usr/local/bin/cb" -o -e "/usr/local/bin/cb-tui" ]; then
        printf '%s\n' "/usr/local/bin"
        return
    fi

    old_ifs=$IFS
    IFS=:
    for dir in $PATH; do
        [ -n "$dir" ] || continue
        case "$dir" in
            */bin)
                if [ -e "$dir/cb" -o -e "$dir/cb-tui" ]; then
                    printf '%s\n' "$dir"
                    IFS=$old_ifs
                    return
                fi
                ;;
        esac
    done
    IFS=$old_ifs

    printf '%s\n' "$HOME/.local/bin"
}

install_bin_dir=$(choose_install_bin_dir)
install_root=$(dirname -- "$install_bin_dir")

case "$install_bin_dir" in
    */bin) ;;
    *)
        echo "error: install bin directory must end with /bin: $install_bin_dir" >&2
        exit 1
        ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
    cat >&2 <<EOF
error: cargo is required to uninstall Code Base from $install_bin_dir.

Install Cargo, then rerun:
  INSTALL_BIN_DIR="$install_bin_dir" ./scripts/uninstall.sh

If you do not want to install Cargo, remove the binaries directly:
  rm -f "$install_bin_dir/cb" "$install_bin_dir/cb-tui"
EOF
    exit 1
fi

if [ ! -e "$install_bin_dir/cb" ] && [ ! -e "$install_bin_dir/cb-tui" ]; then
    echo "Code Base is not installed in $install_bin_dir."
else
    cargo uninstall --root "$install_root" codebase
    echo "Uninstalled Code Base binaries from $install_bin_dir"
fi

cat <<'EOF'

User data and config were not removed.
Default locations are platform-specific and are managed by the app.
If you used overrides, check:
  CODEBASE_DATA_DIR
  CODEBASE_CONFIG_DIR
EOF
