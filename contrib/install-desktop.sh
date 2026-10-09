#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${SIGNALTTY_PREFIX:-"$HOME/.local"}
data="$prefix/share"
icon_src="$repo/crates/signaltty-gui/data/icons/scalable/apps"
icon_dst="$data/icons/hicolor/scalable/apps"

case "${1:-}" in
  install)
    # Desktop sessions often lack ~/.local/bin on PATH, so the entry names
    # the installed binary. Quoting covers spaces; these characters would
    # need Desktop Entry escaping and are refused instead.
    case "$prefix" in
      *[\"\`\$\\%]*|*"$(printf '\t')"*|*"$(printf '\r')"*|*'
'*)
        echo "SIGNALTTY_PREFIX must not contain newline, tab, carriage return, \" \` \$ \\ or %: $prefix" >&2
        exit 2
        ;;
      /*) ;;
      *)
        echo "SIGNALTTY_PREFIX must be an absolute path: $prefix" >&2
        exit 2
        ;;
    esac
    cargo build --release -p signaltty-gui --manifest-path "$repo/Cargo.toml" --target-dir "$repo/target"
    install -Dm755 "$repo/target/release/signaltty-gui" "$prefix/bin/signaltty-gui"
    mkdir -p "$data/applications"
    EXEC_LINE="Exec=\"$prefix/bin/signaltty-gui\"" \
      awk '/^Exec=/ { print ENVIRON["EXEC_LINE"]; next } { print }' \
      "$repo/contrib/dev.signaltty.gui.desktop" >"$data/applications/dev.signaltty.gui.desktop"
    chmod 644 "$data/applications/dev.signaltty.gui.desktop"
    install -Dm644 "$repo/contrib/dev.signaltty.gui.metainfo.xml" "$data/metainfo/dev.signaltty.gui.metainfo.xml"
    install -Dm644 "$icon_src/dev.signaltty.gui.svg" "$icon_dst/dev.signaltty.gui.svg"
    install -Dm644 "$icon_src/dev.signaltty.gui-symbolic.svg" "$icon_dst/dev.signaltty.gui-symbolic.svg"
    ;;
  uninstall)
    rm -f "$prefix/bin/signaltty-gui" \
      "$data/applications/dev.signaltty.gui.desktop" \
      "$data/metainfo/dev.signaltty.gui.metainfo.xml" \
      "$icon_dst/dev.signaltty.gui.svg" \
      "$icon_dst/dev.signaltty.gui-symbolic.svg"
    ;;
  *)
    echo "Usage: $0 install|uninstall" >&2
    exit 2
    ;;
esac
