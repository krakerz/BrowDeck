#!/usr/bin/env bash
# Removes what install.sh installed. Your config
# (~/.config/browdeck/) is left alone — remove it yourself if you want a
# completely clean slate.
set -euo pipefail

bin_dir="$HOME/.local/bin"
apps_dir="$HOME/.local/share/applications"
icon_dir="$HOME/.local/share/icons/hicolor/256x256/apps"

removed=0
for f in "$bin_dir/browdeck" "$apps_dir/browdeck.desktop" "$icon_dir/browdeck.png"; do
    if [ -e "$f" ]; then
        rm -f "$f"
        echo "Removed $f"
        removed=1
    fi
done

command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$apps_dir" 2>/dev/null || true

if [ "$removed" -eq 0 ]; then
    echo "Nothing installed by install.sh was found."
else
    echo "Done. Your config at ~/.config/browdeck/ was left in place."
fi
