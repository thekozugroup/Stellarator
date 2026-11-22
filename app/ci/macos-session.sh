#!/bin/bash
# One launch of the packaged macOS app for CI: start it with the probe on, find its window,
# screenshot the window (with transparent corners) and the screen, read its memory, let the probe
# post its clicks, screenshot again, then quit it the way the user does and say how it ended.
#
#   app/ci/macos-session.sh <name> <Stellarator.app> <data folder> <out folder> <window-id tool> [VAR=value ...]
#
# Writes <out>/<name>.log (the app's output: probe lines and logs), <name>.png (the window),
# <name>-screen.png, <name>-after-clicks.png, <name>-corner.txt and <name>-memory.txt.
set -u
name=$1 app=$2 data=$3 out=$4 window_id=$5
shift 5
exe="$app/Contents/MacOS/Stellarator"
mkdir -p "$out" "$data"

env STELLARATOR_PROBE=1 STELLARATOR_DATA_DIR="$data" RUST_LOG=info "$@" "$exe" > "$out/$name.log" 2>&1 &
pid=$!
wid=""
for _ in $(seq 1 60); do
  if ! kill -0 "$pid" 2> /dev/null; then
    echo "::error::$name: the app exited early"; cat "$out/$name.log"; exit 1
  fi
  wid=$("$window_id" Stellarator 2>> "$out/$name-window.txt") && break
  sleep 1
done
if [ -z "$wid" ]; then
  echo "::error::$name: no window after 60 s"; cat "$out/$name.log"; kill -9 "$pid"; exit 1
fi
echo "$name: window $wid"
# The probe reads AppKit 3 s after the first render; its clicks go out at 10 s.
sleep 6
screencapture -x -o -l "$wid" "$out/$name.png"
screencapture -x "$out/$name-screen.png"
{
  echo "ps rss (KiB): $(ps -o rss= -p "$pid")"
  footprint "$pid" 2>&1 | head -40
} > "$out/$name-memory.txt"
sleep 8
screencapture -x -o -l "$wid" "$out/$name-after-clicks.png"
python3 "$(dirname "$0")/corner.py" "$out/$name.png" 40 > "$out/$name-corner.txt"

# Quit as Cmd+Q does (an Apple event), which goes through the app's quit request: the server
# stops, then the app quits. A runner that refuses Apple events falls back to SIGTERM.
osascript -e 'tell application id "com.thekozugroup.stellarator" to quit' > "$out/$name-quit.txt" 2>&1 &
quitter=$!
for _ in $(seq 1 20); do
  kill -0 "$pid" 2> /dev/null || break
  sleep 1
done
kill "$quitter" 2> /dev/null
if kill -0 "$pid" 2> /dev/null; then
  echo "$name: still running 20 s after the quit request; SIGTERM" | tee -a "$out/$name-quit.txt"
  kill "$pid"; sleep 3; kill -9 "$pid" 2> /dev/null
fi
wait "$pid" 2> /dev/null
echo "$name: exit status $?" | tee -a "$out/$name-quit.txt"
echo "--- $name probe"
grep -E "^probe:|Stellarator server" "$out/$name.log" || true
echo "--- $name corner"
tail -n 41 "$out/$name-corner.txt"
echo "--- $name memory"
head -n 12 "$out/$name-memory.txt"
