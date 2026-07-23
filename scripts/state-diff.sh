#!/usr/bin/env bash
# Differential state-trace harness.
#
# Runs each ROM on two cli builds with `--state-trace` and compares the
# record streams; the first divergence prints both records plus the last match.
# Streams are bounded by an M-cycle, so builds of different speed stay comparable.
#
#   A=<old-cli> B=<new-cli> [MCYCLES=20000000] [INTERVAL=1] [MODEL=auto] \
#       scripts/state-diff.sh <rom> [<rom>...]
#
# Exit code: 0 = all ROMs identical, 1 = at least one divergence.
set -euo pipefail
cd "$(dirname "$0")/.."

A=${A:?usage: A=<cli> B=<cli> scripts/state-diff.sh <rom>...}
B=${B:?usage: A=<cli> B=<cli> scripts/state-diff.sh <rom>...}
MCYCLES=${MCYCLES:-20000000}
INTERVAL=${INTERVAL:-1}
MODEL=${MODEL:-auto}

fail=0
for rom in "$@"; do
    if A="$A" B="$B" ROM="$rom" MCYCLES="$MCYCLES" INTERVAL="$INTERVAL" MODEL="$MODEL" \
        python3 - <<'EOF'
import os, subprocess, sys
from itertools import zip_longest

args = ["run", os.environ["ROM"], "--state-trace",
        "--m-cycles", os.environ["MCYCLES"],
        "--interval", os.environ["INTERVAL"],
        "--model", os.environ["MODEL"]]
pa = subprocess.Popen([os.environ["A"], *args], stdout=subprocess.PIPE)
pb = subprocess.Popen([os.environ["B"], *args], stdout=subprocess.PIPE)

prev = b""
ok = True
for n, (la, lb) in enumerate(zip_longest(pa.stdout, pb.stdout), 1):
    if la != lb:
        print(f"DIVERGED  {os.environ['ROM']}  at record {n}")
        print(f"  last match: {prev.decode().rstrip() if prev else '<none>'}")
        print(f"  A: {la.decode().rstrip() if la else '<end of stream>'}")
        print(f"  B: {lb.decode().rstrip() if lb else '<end of stream>'}")
        ok = False
        pa.kill(); pb.kill()
        break
    prev = la
pa.wait(); pb.wait()
sys.exit(0 if ok else 1)
EOF
    then
        echo "OK        $rom"
    else
        fail=1
    fi
done
exit $fail
