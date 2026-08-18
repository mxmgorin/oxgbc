#!/bin/bash

XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}

if [ -d "/opt/system/Tools/PortMaster/" ]; then
  controlfolder="/opt/system/Tools/PortMaster"
elif [ -d "/opt/tools/PortMaster/" ]; then
  controlfolder="/opt/tools/PortMaster"
elif [ -d "$XDG_DATA_HOME/PortMaster/" ]; then
  controlfolder="$XDG_DATA_HOME/PortMaster"
else
  controlfolder="/roms/ports/PortMaster"
fi

source "$controlfolder/control.txt"
[ -f "${controlfolder}/mod_${CFW_NAME}.txt" ] && source "${controlfolder}/mod_${CFW_NAME}.txt"
get_controls

GAMEDIR=/$directory/ports/oxgbc/

BINNAME="oxgbc.${DEVICE_ARCH:-aarch64}"
# Extracting the zip drops the bit on some CFW.
chmod +x "$GAMEDIR/$BINNAME" 2>/dev/null
if [ ! -x "$GAMEDIR/$BINNAME" ]; then
  echo "ERROR: no runnable oxgbc binary found in $GAMEDIR" >&2
  exit 1
fi
BIN="$GAMEDIR/$BINNAME"

cd "$GAMEDIR"

# One generation back: the launch after a bad one would otherwise erase its log.
mv -f "$GAMEDIR/log.txt" "$GAMEDIR/log.prev.txt" 2>/dev/null
exec > >(tee "$GAMEDIR/log.txt") 2>&1

export HOME="$GAMEDIR"
export XDG_DATA_HOME="$GAMEDIR"
export SDL_GAMECONTROLLERCONFIG="$sdl_controllerconfig"

romsroot="/$directory"
for romsdir in ROMS roms Roms; do
  if [ -d "$romsroot/$romsdir" ]; then
    romsroot="$romsroot/$romsdir"
    break
  fi
done
export OXGBC_ROMS_DIR="$romsroot"
# The shelf lists one folder at a time, and this emulator boots as a Color.
for gbdir in gbc gb gameboy; do
  if [ -d "$romsroot/$gbdir" ]; then
    export OXGBC_ROMS_DIR="$romsroot/$gbdir"
    break
  fi
done
echo "oxgbc: shelving $OXGBC_ROMS_DIR"

#export OXGBC_LOG_LEVEL=debug

$GPTOKEYB "$BINNAME" &
pm_platform_helper "$BIN"
"$BIN"

pm_finish
