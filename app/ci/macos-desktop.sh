#!/bin/bash
# The desktop behavior of the packaged macOS app, for CI: the menu bar, one copy per data folder
# (a second process shows "Stellarator is already open"), Launch Services bringing the running
# copy to the front instead of starting another, and the window opening where it was left
# (nsh.json restoreBounds). It reports what it sees and warns on a difference; it fails only when
# the app does not start.
#
#   app/ci/macos-desktop.sh <Stellarator.app> <data folder> <out folder> <window-id tool> <mouse-drag tool>
#
# Writes <out>/desktop-*.log, desktop-menus.txt, desktop-second-copy.png and desktop-reopened.png.
set -u
app=$1 data=$2 out=$3 window_id=$4 drag=$5
exe="$app/Contents/MacOS/Stellarator"
mkdir -p "$out" "$data"
export STELLARATOR_DATA_DIR="$data" RUST_LOG=info

bounds() { "$window_id" Stellarator 2>&1 > /dev/null | sed -n 's/.*bounds //p'; }
field() { echo "$1" | tr -d '[]"' | tr ',' '\n' | sed -n "s/^ *$2: *//p" | head -n 1; }
copies() { pgrep -f "$exe" | wc -l | tr -d ' '; }
# osascript with a time limit: an Apple event to a busy app can wait for minutes.
apple_event() { perl -e 'alarm 20; exec @ARGV' osascript -e "$1" 2>&1; }
wait_window() {
  for _ in $(seq 1 60); do
    "$window_id" Stellarator > /dev/null 2>&1 && return 0
    kill -0 "$1" 2> /dev/null || return 1
    sleep 1
  done
  return 1
}
quit_app() {
  apple_event 'tell application id "com.thekozugroup.stellarator" to quit' > /dev/null &
  for _ in $(seq 1 20); do kill -0 "$1" 2> /dev/null || return 0; sleep 1; done
  echo "::warning::the app did not quit within 20 s of the quit request; SIGTERM"
  kill "$1" 2> /dev/null; sleep 3; kill -9 "$1" 2> /dev/null
}

"$exe" > "$out/desktop-first.log" 2>&1 &
first=$!
if ! wait_window "$first"; then
  echo "::error::the app did not open its window"; cat "$out/desktop-first.log"; kill -9 "$first" 2> /dev/null; exit 1
fi
sleep 3

# The window, moved by its top band.
before=$(bounds)
x=$(field "$before" X); y=$(field "$before" Y)
"$drag" "$(echo "$x + 140" | bc)" "$(echo "$y + 26" | bc)" "$(echo "$x + 290" | bc)" "$(echo "$y + 116" | bc)"
sleep 1
moved=$(bounds)
echo "window at launch $before, after the drag $moved"

# The menu bar, through System Events (needs the Accessibility permission the hosted runners give).
{
  echo "menu bar: $(apple_event 'tell application "System Events" to tell process "Stellarator" to get name of every menu bar item of menu bar 1')"
  for menu in Stellarator Edit View Window Help; do
    echo "$menu: $(apple_event "tell application \"System Events\" to tell process \"Stellarator\" to get name of every menu item of menu 1 of menu bar item \"$menu\" of menu bar 1")"
  done
} | tee "$out/desktop-menus.txt"
grep -q "Settings" "$out/desktop-menus.txt" || echo "::warning::no Settings item read from the Stellarator menu (see desktop-menus.txt)"

# A second process on the same data folder: it shows why it cannot start, then quits from there.
"$exe" > "$out/desktop-second.log" 2>&1 &
second=$!
sleep 8
screencapture -x "$out/desktop-second-copy.png"
if grep -q "already using the data folder" "$out/desktop-second.log"; then
  echo "second copy: refused the data folder, as expected"
else
  echo "::warning::the second copy did not report the data folder in use"; tail -5 "$out/desktop-second.log"
fi
kill "$second" 2> /dev/null; sleep 2; kill -9 "$second" 2> /dev/null

# Opening the app again through Launch Services (as from the Finder or the Dock) brings the running
# copy forward.
open "$app"
sleep 6
echo "copies after open: $(copies)"
[ "$(copies)" = 1 ] || echo "::warning::open started another copy"
for pid in $(pgrep -f "$exe"); do [ "$pid" = "$first" ] || kill -9 "$pid" 2> /dev/null; done

quit_app "$first"
wait "$first" 2> /dev/null
echo "first copy: exit status $?"

# Started again, the window is where it was left.
"$exe" > "$out/desktop-reopened.log" 2>&1 &
again=$!
if wait_window "$again"; then
  sleep 3
  reopened=$(bounds)
  screencapture -x "$out/desktop-reopened.png"
  echo "window when started again $reopened"
  [ "$(field "$reopened" X)" = "$(field "$moved" X)" ] && [ "$(field "$reopened" Y)" = "$(field "$moved" Y)" ] \
    || echo "::warning::the window did not open where it was left"
else
  echo "::warning::the app did not open its window the second time"; tail -5 "$out/desktop-reopened.log"
fi
quit_app "$again"
exit 0
