#!/bin/sh
set -eu

description=$(sed -n '/^Description:/,/^[^ ]/p' debian/control)

printf '%s\n' "$description" | grep -Fqi 'desktop backends'
printf '%s\n' "$description" | grep -Fqi 'sway'
if printf '%s\n' "$description" | grep -Fqi 'gsettings'; then
  exit 1
fi
