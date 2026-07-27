#!/bin/sh
set -eu

description=$(sed -n '/^Description:/,/^[^ ]/p' debian/control)

printf '%s\n' "$description" | grep -Fqi 'desktop backends'
printf '%s\n' "$description" | grep -Fqi 'GNOME'
printf '%s\n' "$description" | grep -Fqi 'COSMIC'
printf '%s\n' "$description" | grep -Fqi 'sway'
printf '%s\n' "$description" | grep -Fqi 'D-Bus'
if printf '%s\n' "$description" | grep -Fqi 'gsettings'; then
  exit 1
fi

grep -Eq '^default = \["gnome"\]$' Cargo.toml
grep -Eq '^gnome = \["dep:gio", "dep:glib"\]$' Cargo.toml
grep -Eq '^cosmic = \["dep:cosmic-config", "dep:notify"\]$' Cargo.toml
grep -Eq '^cosmic-config = .*optional = true' Cargo.toml
grep -Eq '^notify = .*optional = true' Cargo.toml
test -f src/backend.rs
test -f src/cosmic.rs
