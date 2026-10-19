#!/bin/sh
# Builds the popup library sources together with the preview harness and runs it.
#
#   ./run.sh [--dark|--light] [--filter TEXT] [--private] [--select N] [--url URL] [--source APP]
#            [--keys "c,h,Down,Return"] [--update|--reshow [--then KEYS]] [--click ROW]
#            [--context ROW] [--menu] [--menu-keys KEYS]
#            [--many] [--none] [--no-hotkeys] [--backdrop] [--seconds N]
#
# --keys takes comma-separated tokens: single characters, digits, Return, Enter, Esc, Space,
# Backspace, Up, Down, Shift / -Shift (press / release), Cmd+<char> (Cmd+comma for ⌘,).
# Screenshot while it runs: screencapture -o -x -l <WINDOW number it prints> out.png
set -e
cd "$(dirname "$0")"
swiftc -target arm64-apple-macos13.0 ../*.swift main.swift -o /tmp/popup-preview
exec /tmp/popup-preview "$@"
