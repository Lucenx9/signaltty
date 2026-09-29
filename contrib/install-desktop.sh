#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${SIGNALTTY_PREFIX:-"$HOME/.local"}
data="$prefix/share"
icon_src="$repo/crates/signaltty-gui/data/icons/scalable/apps"
icon_dst="$data/icons/hicolor/scalable/apps"

case "${1:-}" in
  install)
    cargo build --release -p signaltty-gui --manifest-path "$repo/Cargo.toml" --target-dir "$repo/target"
    install -Dm755 "$repo/target/release/signaltty-gui" "$prefix/bin/signaltty-gui"
    install -Dm644 "$repo/contrib/dev.signaltty.gui.desktop" "$data/applications/dev.signaltty.gui.desktop"
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
