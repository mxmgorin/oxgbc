#!/usr/bin/env bash
# Assemble the PortMaster package from cross-built binaries, and zip it.
#
#   tools/package.sh              # into target/
#   tools/package.sh somewhere/   # somewhere other than target/
#
# The binaries come from OXGBC_ARMHF and OXGBC_AARCH64 where those are set, which
# is how CI hands over its build artifacts; anything unset is cross-built here.
# `.github/workflows/build-linux-arm.yml` calls this too, so the layout is
# described in one place rather than in two that drift.
set -euo pipefail

out=${1:-target}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)

mkdir -p "$out"
out=$(cd "$out" && pwd)

# The binary for an arch: whatever was handed over, else one built the way CI
# builds it. `build.sh` prints the path as its last line.
binary() {
  local arch=$1 given
  case $arch in
  armhf) given=${OXGBC_ARMHF:-} ;;
  aarch64) given=${OXGBC_AARCH64:-} ;;
  esac
  if [ -n "$given" ]; then
    printf '%s\n' "$given"
    return
  fi
  echo "no $arch binary given; cross-building one" >&2
  "$here/arm/build.sh" "$arch" | tail -1
}

# The zip a release carries, and its checksum. Written by hand rather than with
# `zip`, which is not everywhere: the mode has to survive the trip, or the
# binary and the launcher arrive on the card unexecutable.
archive() {
  local tree=$1 zip=$2
  rm -f "$zip" "$zip.sha256"
  python3 - "$tree" "$zip" <<'PY'
import os, pathlib, sys, zipfile

root, name = pathlib.Path(sys.argv[1]), sys.argv[2]
with zipfile.ZipFile(name, "w", zipfile.ZIP_DEFLATED) as z:
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        entry = zipfile.ZipInfo(str(path.relative_to(root)))
        entry.external_attr = (os.stat(path).st_mode & 0xFFFF) << 16
        entry.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(entry, path.read_bytes())
PY
  (cd "$(dirname "$zip")" && sha256sum "$(basename "$zip")" >"$(basename "$zip").sha256")
}

armhf=$(binary armhf)
aarch64=$(binary aarch64)

pm=$out/portmaster-dist
game=$pm/oxgbc
rm -rf "$pm"
mkdir -p "$game"
# Only the launcher sits at the port root; everything else lives in the oxgbc/
# gamedir, which is the layout an installed port has.
cp "$repo/portmaster/oxGBC.sh" "$pm/"
for file in port.json gameinfo.xml README.md screenshot.png; do
  cp "$repo/portmaster/oxgbc/$file" "$game/"
done
cp -r "$repo/portmaster/oxgbc/licenses" "$game/"
# Both arches ride along: the launcher picks by what the device reports.
install -m 755 "$aarch64" "$game/oxgbc.aarch64"
install -m 755 "$armhf" "$game/oxgbc.armhf"
chmod 755 "$pm/oxGBC.sh"
archive "$pm" "$out/oxgbc-portmaster.zip"

ls -la "$out"/oxgbc-portmaster.zip*
