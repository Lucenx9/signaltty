#!/bin/sh
# Log hook: stdin is the event envelope JSON.
# Install: cp -r examples/plugins/event-log ~/.config/signaltty/plugins/
#          signaltty plugin reload
echo "$SIGNALTTY_EVENT :: $(cat)" >> "${EVENT_LOG:-/tmp/signaltty-events.log}"
