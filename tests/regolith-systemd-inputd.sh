#!/usr/bin/env bash
set -euo pipefail

unit=data/regolith-init-inputd.service
unit_section=$(sed -n '/^\[Unit\]$/,/^\[Service\]$/p' "$unit")
install_section=$(sed -n '/^\[Install\]$/,$p' "$unit")

grep -Fxq "PartOf=graphical-session.target" <<<"$unit_section"
grep -Fxq "After=graphical-session.target" <<<"$unit_section"
grep -Fxq "StartLimitIntervalSec=10" <<<"$unit_section"
grep -Fxq "StartLimitBurst=5" <<<"$unit_section"
! grep -Fq "Wants=gnome-session.target" <<<"$unit_section"
! grep -Fq "WantedBy=regolith-wayland.target" <<<"$install_section"
grep -Fxq "WantedBy=regolith-gnome.target regolith-cosmic.target" <<<"$install_section"

grep -Fxq "ExecStart=/usr/bin/regolith-inputd" data/regolith-init-inputd.service
grep -Fxq "Restart=on-failure" data/regolith-init-inputd.service
grep -Fxq "data/regolith-init-inputd.service /usr/lib/systemd/user/" debian/install
