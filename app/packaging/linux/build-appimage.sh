#!/bin/bash
# The AppImage of the nsh app: the packaged app (nsh package --target linux) with an AppRun, the
# desktop file and the icon. It uses the computer's GTK 4 (4.14 or later: Ubuntu 24.04, Debian 13,
# Fedora 40 and newer), as the .deb does; GTK is not bundled.
#
#   app/packaging/linux/build-appimage.sh <app dir (.nsh/out/Stellarator)> <out .AppImage>
set -euo pipefail
app=$1 out=$2
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
id=com.thekozugroup.stellarator
work=$(mktemp -d)
dir="$work/Stellarator.AppDir"
mkdir -p "$dir/usr/lib/stellarator" "$dir/usr/share/metainfo"
cp -a "$app/." "$dir/usr/lib/stellarator/"
rm -f "$dir/usr/lib/stellarator/$id.desktop" "$dir/usr/lib/stellarator/$id.png"
cat > "$dir/AppRun" <<'RUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/lib/stellarator/Stellarator" "$@"
RUN
chmod +x "$dir/AppRun"
sed 's/^Exec=.*/Exec=stellarator %U/' "$here/$id.desktop" > "$dir/$id.desktop"
convert "$repo/app/icons/icon.png" -resize 256x256 "$dir/$id.png"
cp "$dir/$id.png" "$dir/.DirIcon"
cp "$repo/store/linux/$id.metainfo.xml" "$dir/usr/share/metainfo/$id.appdata.xml"
tool="$work/appimagetool"
curl -fsSL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x "$tool"
# No FUSE on the build machine: the tool runs from its extracted self.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$dir" "$out"
rm -rf "$work"
ls -la "$out"
