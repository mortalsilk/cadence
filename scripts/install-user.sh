#!/usr/bin/env bash
set -euo pipefail
package=${1:?Usage: install-user.sh path/to/cadence.deb}
[[ -f "$package" ]] || { echo "Package not found: $package" >&2; exit 1; }
[[ "$(dpkg-deb -f "$package" Package)" == "cadence" ]] || { echo 'This is not a cadence package.' >&2; exit 1; }
app_root="${XDG_DATA_HOME:-$HOME/.local/share}/cadence-app"
bin_root="$HOME/.local/bin"
launcher_root="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$app_root" "$bin_root" "$launcher_root"
dpkg-deb -x "$package" "$app_root"
ln -sfn "$app_root/usr/bin/cadence" "$bin_root/cadence"
# Quote desktop-entry values so spaces in a home directory remain usable.
python3 - "$app_root" "$launcher_root" <<'PY'
import pathlib,sys
root=pathlib.Path(sys.argv[1]); destination=pathlib.Path(sys.argv[2])/'cadence.desktop'
quote=lambda s:s.replace('\\','\\\\').replace('"','\\"').replace('`','\\`').replace('$','\\$').replace('%','%%')
icon=next((root/'usr/share/icons').rglob('*.png'))
destination.write_text('[Desktop Entry]\nType=Application\nName=cadence\nComment=Local stint and session tracker\nExec="'+quote(str(root/'usr/bin/cadence'))+'"\nIcon='+str(icon)+'\nTerminal=false\nCategories=Office;Utility;\nStartupWMClass=cadence\n')
PY
if command -v update-desktop-database >/dev/null; then update-desktop-database "$launcher_root"; fi
printf 'Installed cadence for this user: %s\n' "$bin_root/cadence"
