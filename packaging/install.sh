#!/usr/bin/env bash
# Installs BrowDeck (binary + desktop entry + icon) for the current user.
# Run from inside the extracted release archive, next to this script:
#   ./install.sh
set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin_dir="$HOME/.local/bin"
apps_dir="$HOME/.local/share/applications"
icon_dir="$HOME/.local/share/icons/hicolor/256x256/apps"
config_dir="$HOME/.config/browdeck"

if [ ! -f "$dir/browdeck" ]; then
    echo "error: browdeck binary not found next to this script ($dir)" >&2
    exit 1
fi

mkdir -p "$bin_dir" "$apps_dir" "$icon_dir" "$config_dir"

install -m 755 "$dir/browdeck" "$bin_dir/browdeck"
install -m 644 "$dir/browdeck.desktop" "$apps_dir/browdeck.desktop"
install -m 644 "$dir/icon.png" "$icon_dir/browdeck.png"

# Copied as .example, never overwriting a real config.toml — see
# config.example.toml's own comments for how to use it.
install -m 644 "$dir/config.example.toml" "$config_dir/config.example.toml"

command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$apps_dir" 2>/dev/null || true

echo "Installed to $bin_dir/browdeck"

case ":$PATH:" in
*":$bin_dir:"*) ;;
*)
    echo "Note: $bin_dir isn't on your \$PATH — add it in your shell's rc file"
    echo "  (e.g. export PATH=\"\$HOME/.local/bin:\$PATH\") to run 'browdeck' directly."
    ;;
esac

echo "Done. Run with: browdeck"
echo "To add BrowDeck to Steam Game Mode, add it as a Non-Steam Game pointing at $bin_dir/browdeck."
