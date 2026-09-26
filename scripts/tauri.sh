#!/bin/sh
# Runs the Tauri CLI with Rust on the PATH, even in a Terminal whose profile doesn't load ~/.cargo/env.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust (cargo) isn't installed. Install it with:  curl https://sh.rustup.rs -sSf | sh" >&2
  exit 1
fi
exec npx --no-install tauri "$@"
