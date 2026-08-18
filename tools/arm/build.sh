#!/usr/bin/env bash
# Cross-build an ARM binary the way CI does, and print its path.
#
#   tools/arm/build.sh armhf|aarch64 [cargo args...]
#
# Caches live outside the repo so container-root files never mix with host
# builds.
set -euo pipefail

arch=${1:?usage: build.sh armhf|aarch64 [cargo args...]}
shift || true

case "$arch" in
armhf)
  target=armv7-unknown-linux-gnueabihf
  libdir=/opt/sdl2/armhf/usr/lib/arm-linux-gnueabihf
  ;;
aarch64)
  target=aarch64-unknown-linux-gnu
  libdir=/opt/sdl2/arm64/usr/lib/aarch64-linux-gnu
  ;;
*)
  echo "unknown arch: $arch (expected armhf or aarch64)" >&2
  exit 2
  ;;
esac

image=oxgbc-arm-cross
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
cache=${OXGBC_ARM_CACHE:-$HOME/.cache/oxgbc-arm}
# The glibc floor, from the oldest userland we target (ArkOS, AmberELEC).
floor=$(cat "$here/glibc-floor")
# What a handheld build is, shared with CI: the egui frontend, the in-app file
# browser instead of a desktop dialog that shells out to zenity, and the device's
# own SDL2 rather than a bundled one — every CFW patches SDL2 for its panel and pad.
args=$(cat "$here/cargo-args")

mkdir -p "$cache/target" "$cache/cargo" "$cache/zig"
# Cached after the first run.
docker build -q -t "$image" -f "$here/Dockerfile" "$here" >/dev/null

docker run --rm --network host \
  -v "$repo":/repo \
  -v "$cache/target":/target \
  -v "$cache/cargo":/cargo \
  -v "$cache/zig":/zig-cache \
  -e CARGO_TARGET_DIR=/target -e CARGO_HOME=/cargo -e RUSTUP_HOME=/cargo \
  -e ZIG_GLOBAL_CACHE_DIR=/zig-cache \
  -e "TARGET=$target" -e "LIBDIR=$libdir" -e "FLOOR=$floor" \
  -e "HOST_UID=$(id -u)" -e "HOST_GID=$(id -g)" \
  -e "BUILD_ARGS=$args" -e "CARGO_ARGS=$*" \
  "$image" bash -euxc '
    export PATH="/cargo/bin:$PATH"
    command -v cargo >/dev/null || curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --default-toolchain stable
    cd /repo
    # The build stamps the commit it came from; without this git refuses a tree
    # owned by another user and the About page reads `unknown`.
    git config --global --add safe.directory /repo
    rustup target add "$TARGET"
    # No linker override: cargo-zigbuild points the linker and cc at zig itself,
    # and a -C linker of our own would opt out of it.
    # --allow-shlib-undefined: this libSDL2 pulls in X11/wayland/alsa/gbm, which
    # the sysroot lacks and we never call.
    export RUSTFLAGS="-L $LIBDIR -C link-arg=-Wl,--allow-shlib-undefined"
    cargo zigbuild --release --target "$TARGET.$FLOOR" $BUILD_ARGS $CARGO_ARGS
    out="/target/$TARGET/release/oxgbc"
    readelf -V "$out" | grep -o "GLIBC_2\.[0-9]*" | sort -uV | tr "\n" " "
    echo
    # Above the floor the loader on the device refuses it, so fail here instead.
    major_floor=${FLOOR#2.}
    newer=$(readelf -V "$out" | grep -o "GLIBC_2\.[0-9]*" | sort -uV \
      | awk -F. -v f="$major_floor" "\$2 > f" | tr "\n" " ")
    [ -z "$newer" ] || { echo "binary requires $newer; the floor is GLIBC_$FLOOR" >&2; exit 1; }
    chown -R "$HOST_UID:$HOST_GID" /target /cargo /zig-cache
  '

echo "$cache/target/$target/release/oxgbc"
