#!/bin/bash
# The .deb of the nsh app: the packaged app (nsh package --target linux) under /usr/lib/stellarator,
# `stellarator` on the PATH, the desktop file, the icons and the AppStream metainfo
# (store/linux/). The dependencies are what the executable links (dpkg-shlibdeps): GTK 4.14 or
# later, so Ubuntu 24.04 or Debian 13 and newer.
#
#   app/packaging/linux/build-deb.sh <app dir (.nsh/out/Stellarator)> <version> <out .deb>
set -euo pipefail
app=$1 version=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
id=com.thekozugroup.stellarator
work=$(mktemp -d)
root="$work/pkg"
mkdir -p "$root/usr/lib/stellarator" "$root/usr/bin" "$root/usr/share/applications" "$root/usr/share/metainfo" "$root/DEBIAN"
cp -a "$app/." "$root/usr/lib/stellarator/"
# nsh's own desktop file and icon are replaced by the ones below.
rm -f "$root/usr/lib/stellarator/$id.desktop" "$root/usr/lib/stellarator/$id.png"
ln -s ../lib/stellarator/Stellarator "$root/usr/bin/stellarator"
cp "$here/$id.desktop" "$root/usr/share/applications/"
cp "$repo/store/linux/$id.metainfo.xml" "$root/usr/share/metainfo/"
for size in 32 48 64 128 256 512; do
  dir="$root/usr/share/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  convert "$repo/app/icons/icon.png" -resize "${size}x${size}" "$dir/$id.png"
done
# What the executable needs, from the libraries it links.
mkdir -p "$work/debian"
printf 'Source: stellarator\n\nPackage: stellarator\nArchitecture: amd64\n' > "$work/debian/control"
depends=$(cd "$work" && dpkg-shlibdeps -O "pkg/usr/lib/stellarator/Stellarator" 2> /dev/null | sed -n 's/^shlibs:Depends=//p')
size=$(du -sk "$root/usr" | cut -f1)
cat > "$root/DEBIAN/control" <<CONTROL
Package: stellarator
Version: $version
Architecture: amd64
Maintainer: The Kozu Group <https://github.com/thekozugroup/Stellarator>
Installed-Size: $size
Depends: $depends
Section: devel
Priority: optional
Homepage: https://github.com/thekozugroup/Stellarator
Description: Distill datasets from LLMs
 Stellarator sends a set of prompts to a model server (anything with the OpenAI chat
 completions API), keeps each answer with its thinking trace, and exports the result as a
 dataset for fine-tuning or preference training.
CONTROL
dpkg-deb --root-owner-group --build "$root" "$out"
rm -rf "$work"
dpkg-deb --info "$out" | sed -n '/Package:/,$p'
