#!/bin/bash
# One launch of the packaged Linux app for CI, on a virtual display: start it with the probe on,
# screenshot the screen, read its memory, then stop it.
#
#   xvfb-run -a -s "-screen 0 1440x900x24" app/ci/linux-session.sh <name> <executable> <data folder> <out folder> [VAR=value ...]
set -u
name=$1 exe=$2 data=$3 out=$4
shift 4
mkdir -p "$out" "$data"
env STELLARATOR_PROBE=1 STELLARATOR_DATA_DIR="$data" RUST_LOG=info GSK_RENDERER=cairo "$@" "$exe" > "$out/$name.log" 2>&1 &
pid=$!
sleep 12
import -window root "$out/$name.png"
{
  echo "ps rss (KiB): $(ps -o rss= -p "$pid")"
  grep -E "^(Rss|Pss|Private_Dirty|Private_Clean)" "/proc/$pid/smaps_rollup" 2> /dev/null
} > "$out/$name-memory.txt"
kill "$pid" 2> /dev/null
sleep 3
kill -9 "$pid" 2> /dev/null
echo "--- $name"
grep -E "^probe:|Stellarator server|first_frame" "$out/$name.log" || tail -n 30 "$out/$name.log"
cat "$out/$name-memory.txt"
