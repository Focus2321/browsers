#!/bin/sh
# Builds the BrowsersPopup sources (popup + Settings/About) with the Settings preview harness and
# runs it.
#
#   ./run.sh [--page browsers|rules|general|about] [--dark|--light] [--about] [--standard-about]
#            [--seconds N] [--script "STEP;STEP;..."] [--no-answer] [--no-hidden] [--no-banner]
#            [--default] [--draft] [--test-url URL] [--shots DIR]
#
# Script steps, run 0.35 s apart (wait:S adds time):
#   click:X,Y            click at X,Y in the key window, top-left window coordinates
#   keys:TOKENS          comma-separated keys: characters, Return, Esc, Tab, Space, Backspace,
#                        Up, Down, Left, Right, Cmd+<char>, CmdShift+<char>
#   type:TEXT            types TEXT into whatever has focus
#   menu:TOKENS          keys queued for a menu a click is about to open
#   update               sends browsers_settings_update with the current sample state
#   show[:PAGE]          browsers_settings_show (no page = keep the current one)
#   close                browsers_settings_close
#   about | about-update | about-close
#   theme:system|light|dark   changes the theme the sample state reports, then updates
#   shot:NAME            screencapture -o -x -l of the key window into DIR/NAME.png
#   focus                prints the key window's first responder (and the field's text)
#   scroll:X,Y,DY        scrolls what's under X,Y by DY points (positive = down)
#   is-open | origin | toolbar
#
# Output: "WINDOW <number>", "ORIGIN <x>,<y> SIZE <w>x<h>" (top-left screen coordinates),
# "EVENT <json>" per event, "STEP", "SHOT", "FOCUS", "OPEN" lines.
set -e
cd "$(dirname "$0")"
swiftc -target arm64-apple-macos13.0 ../*.swift main.swift -o /tmp/settings-preview
exec /tmp/settings-preview "$@"
