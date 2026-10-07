#!/bin/sh
# Remove everything a signaltty install adds for the current user: agent hooks
# and skills, the systemd user service, the desktop app, and the server and CLI
# binaries. --purge also deletes saved state and configuration (plugins too).
set -eu

repo=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${SIGNALTTY_PREFIX:-"$HOME/.local"}
cargo_root=${CARGO_HOME:-"$HOME/.cargo"}
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
state_home=${XDG_STATE_HOME:-"$HOME/.local/state"}
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}

case "${1:-}" in
  "") purge=0 ;;
  --purge) purge=1 ;;
  *)
    echo "Usage: $0 [--purge]" >&2
    exit 2
    ;;
esac

# Hooks and skills go first: removing them needs the CLI. A refusal (e.g. a
# malformed agent settings file) stops here, before anything else is removed.
cli=
for candidate in "$prefix/bin/signaltty" "$cargo_root/bin/signaltty"; do
  if [ -x "$candidate" ]; then
    cli=$candidate
    break
  fi
done
[ -n "$cli" ] || cli=$(command -v signaltty || true)
if [ -n "$cli" ]; then
  "$cli" integration uninstall all
  "$cli" skill uninstall
else
  echo "signaltty CLI not found; agent hooks and skills were left in place" >&2
fi

unit_dir="$config_home/systemd/user"
if [ -e "$unit_dir/signaltty-server.service" ]; then
  systemctl --user disable --now signaltty-server.service || true
  rm -f "$unit_dir/signaltty-server.service" \
    "$unit_dir/default.target.wants/signaltty-server.service"
  systemctl --user daemon-reload || true
fi

"$repo/contrib/install-desktop.sh" uninstall

# The README installs with cargo into $CARGO_HOME; the desktop flow uses the
# prefix. Let cargo drop its own records where it tracks the package.
for root in "$prefix" "$cargo_root"; do
  for pkg_bin in signaltty-server:signaltty-server signaltty-cli:signaltty; do
    pkg=${pkg_bin%%:*}
    bin=${pkg_bin#*:}
    if command -v cargo >/dev/null && [ -f "$root/.crates.toml" ] &&
      grep -q "^\"$pkg " "$root/.crates.toml"; then
      cargo uninstall --quiet --root "$root" "$pkg"
    fi
    rm -f "$root/bin/$bin"
  done
done

if [ "$purge" = 1 ]; then
  rm -rf "$state_home/signaltty" "$config_home/signaltty"
fi

# Worktrees hold agents' checkouts, so they are never deleted here.
if [ -d "$data_home/signaltty/worktrees" ]; then
  echo "kept agent worktrees in $data_home/signaltty/worktrees; remove them with git worktree remove" >&2
fi
# Match the command line: the kernel truncates process names to 15 characters.
if pgrep -u "$(id -u)" -f '(^|/)signaltty-server( |$)' >/dev/null 2>&1; then
  echo "a signaltty-server process is still running; stop it to release its socket" >&2
fi
