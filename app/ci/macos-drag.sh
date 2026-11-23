#!/bin/bash
# Drags the packaged app's window by its top band with the system's mouse (CGEvent), over the
# sidebar and over the toolbar, and says whether the window moved. The app runs with
# STELLARATOR_PROBE=drag, so the band has no press handlers of the probe.
#
#   app/ci/macos-drag.sh <Stellarator.app> <data folder> <out folder> <window-id tool> <mouse-drag tool>
set -u
app=$1 data=$2 out=$3 window_id=$4 drag=$5
exe="$app/Contents/MacOS/Stellarator"
mkdir -p "$out" "$data"
env STELLARATOR_PROBE=drag STELLARATOR_DATA_DIR="$data" RUST_LOG=info "$exe" > "$out/drag.log" 2>&1 &
pid=$!
bounds() { "$window_id" Stellarator 2>&1 > /dev/null | sed -n 's/.*bounds //p'; }
field() { echo "$1" | tr -d '[]"' | tr ',' '\n' | sed -n "s/^ *$2: *//p" | head -n 1; }
for _ in $(seq 1 60); do "$window_id" Stellarator > /dev/null 2>&1 && break; sleep 1; done
sleep 5
for spot in "sidebar 140 26 200 100" "toolbar 600 26 -150 60"; do
  set -- $spot
  before=$(bounds)
  x=$(field "$before" X); y=$(field "$before" Y)
  sx=$(echo "$x + $2" | bc); sy=$(echo "$y + $3" | bc)
  "$drag" "$sx" "$sy" "$(echo "$sx + $4" | bc)" "$(echo "$sy + $5" | bc)"
  sleep 1
  after=$(bounds)
  echo "drag over the $1 band by ($4, $5): window before $before, after $after"
done
screencapture -x "$out/drag-screen.png"
kill "$pid" 2> /dev/null; sleep 3; kill -9 "$pid" 2> /dev/null
grep -E "^probe: (toolbar attached|hitTest \(140, 26\))" "$out/drag.log" || true
