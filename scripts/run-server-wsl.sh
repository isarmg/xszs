#!/usr/bin/env bash
set -euo pipefail
[[ "$EUID" -eq 0 ]] || { echo "run as root (or with sudo)" >&2; exit 1; }
systemctl start xszs.service
exec journalctl --utc --unit xszs.service --follow
