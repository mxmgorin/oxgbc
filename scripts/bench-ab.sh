#!/usr/bin/env bash
# Interleaved A/B bench matrix.
#
# Runs the pinned workload matrix against one or two binaries,
# strictly alternating A/B runs within each workload (the machine drifts up
# to 20% between series — never compare runs from different time windows),
# and reports per-workload medians.
#
#   A=path/to/cli [B=path/to/cli] [PAIRS=5] scripts/bench-ab.sh
#
# With B unset, reports absolute medians for A only.
#
# Workloads: in-repo test ROMs always run; real games are added when the env
# vars point at them:
#   OXGBC_BENCH_GB_ROM   — a DMG game
#   OXGBC_BENCH_GBC_ROM  — a CGB game (double speed, busy audio)
set -euo pipefail
cd "$(dirname "$0")/.."

A=${A:?usage: A=<cli> [B=<cli>] [PAIRS=5] scripts/bench-ab.sh}
B=${B:-}
PAIRS=${PAIRS:-5}

NAMES=()
CMDS=()
# %q-quote every arg so game paths with spaces survive the eval round-trip
add() { NAMES+=("$1"); shift; CMDS+=("$(printf '%q ' "$@")"); }

add cpu_instrs-600f   bench roms/cpu_instrs.gb --frames 600
add same-suite-apu    check roms/same-suite/apu -r --timeout 5
[ -n "${OXGBC_BENCH_GB_ROM:-}" ]  && add gb-game-1200f  bench "$OXGBC_BENCH_GB_ROM" --frames 1200
[ -n "${OXGBC_BENCH_GBC_ROM:-}" ] && add gbc-game-1200f bench "$OXGBC_BENCH_GBC_ROM" --frames 1200

# Wall-clock one run; `check` legitimately exits non-zero on known failures.
run_one() { # $1=binary, rest=args
    local bin=$1 t0 t1
    shift
    t0=$(date +%s.%N)
    "$bin" "$@" >/dev/null 2>&1 || true
    t1=$(date +%s.%N)
    awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.3f", b - a }'
}

median() { # newline-separated numbers on stdin
    sort -n | awk '{ v[NR] = $1 } END {
        if (NR % 2) print v[(NR + 1) / 2];
        else printf "%.3f\n", (v[NR / 2] + v[NR / 2 + 1]) / 2 }'
}

echo "workload            A-median   B-median   delta"
echo "------------------  ---------  ---------  ------"

for i in "${!NAMES[@]}"; do
    name=${NAMES[$i]}
    eval "args=(${CMDS[$i]})"

    ta=()
    tb=()
    run_one "$A" "${args[@]}" >/dev/null # warm-up
    [ -n "$B" ] && run_one "$B" "${args[@]}" >/dev/null

    for _ in $(seq "$PAIRS"); do
        ta+=("$(run_one "$A" "${args[@]}")")
        [ -n "$B" ] && tb+=("$(run_one "$B" "${args[@]}")")
    done

    ma=$(printf '%s\n' "${ta[@]}" | median)
    if [ -n "$B" ]; then
        mb=$(printf '%s\n' "${tb[@]}" | median)
        delta=$(awk -v a="$ma" -v b="$mb" 'BEGIN { printf "%+.1f%%", (b - a) / a * 100 }')
        printf '%-18s  %8ss  %8ss  %s\n' "$name" "$ma" "$mb" "$delta"
    else
        printf '%-18s  %8ss\n' "$name" "$ma"
    fi
done
