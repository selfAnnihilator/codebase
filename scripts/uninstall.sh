#!/bin/sh
set -eu

if ! command -v cargo >/dev/null 2>&1; then
    cat >&2 <<'EOF'
error: cargo is required to uninstall Code Base when it was installed with scripts/install.sh.

Install Cargo, then rerun:
  ./scripts/uninstall.sh

If you manually removed Cargo, you can remove the binaries directly from Cargo's bin directory:
  rm -f "$HOME/.cargo/bin/cb" "$HOME/.cargo/bin/cb-tui"
EOF
    exit 1
fi

if ! cargo install --list | grep -q '^codebase '; then
    echo "Code Base is not installed by Cargo."
else
    cargo uninstall codebase
    echo "Uninstalled Code Base binaries: cb, cb-tui"
fi

cat <<'EOF'

User data and config were not removed.
Default locations are platform-specific and are managed by the app.
If you used overrides, check:
  CODEBASE_DATA_DIR
  CODEBASE_CONFIG_DIR
EOF
