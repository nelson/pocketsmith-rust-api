#!/usr/bin/env bash
#
# In-VM bootstrap for pocketsmith. Vendor-agnostic: run this on a fresh
# exe.dev / boxd.sh / sprites.dev VM. Downloads the latest release tarball,
# installs the binaries + rules, writes systemd units, and starts `serve`.
#
# The only per-vendor difference is SERVE_PORT (front-door target):
#   exe.dev -> 3141   boxd.sh -> 8000   sprites.dev -> 8080
#
# /data/pocketsmith.env MUST already exist with at least POCKETSMITH_API_KEY
# (and optionally GOOGLE_PLACES_API_KEY) before running.
set -euo pipefail

REPO="${REPO:-nelson/pocketsmith-rust-api}"   # GitHub owner/repo
PORT="${SERVE_PORT:-3141}"
TARBALL="pocketsmith-x86_64-linux-musl.tar.gz"

sudo mkdir -p /data /opt/pocketsmith

echo "==> Downloading latest release from $REPO"
curl -fsSL "https://github.com/$REPO/releases/latest/download/$TARBALL" \
  | sudo tar -xz -C /opt/pocketsmith

echo "==> Installing binary"
# The toolkit now ships as a single `pocketsmith` binary; every former command
# (sync, transfers, normalise, serve, push, dump, rule) is a subcommand.
if [ -f /opt/pocketsmith/pocketsmith ]; then
  sudo install /opt/pocketsmith/pocketsmith /usr/local/bin/pocketsmith
else
  echo "ERROR: pocketsmith binary missing from release tarball" >&2
  exit 1
fi

if [ ! -f /data/pocketsmith.env ]; then
  echo "ERROR: /data/pocketsmith.env missing (needs POCKETSMITH_API_KEY)" >&2
  exit 1
fi

echo "==> Writing /etc/default/pocketsmith (SERVE_PORT=$PORT)"
cat <<EOF | sudo tee /etc/default/pocketsmith >/dev/null
POCKETSMITH_DB=/data/pocketsmith.db
POCKETSMITH_RULES_DIR=/opt/pocketsmith/rules
SERVE_HOST=0.0.0.0
SERVE_PORT=$PORT
EOF

echo "==> Installing systemd units"
sudo cp deploy/systemd/pocketsmith-serve.service     /etc/systemd/system/
sudo cp deploy/systemd/pocketsmith-pipeline.service  /etc/systemd/system/
sudo cp deploy/systemd/pocketsmith-pipeline.timer    /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pocketsmith-serve.service

# The timer fires only on always-on VMs (exe.dev / boxd). On sprites the VM
# sleeps, so the timer never fires; scheduling is driven externally by the
# GitHub Actions workflow in .github/workflows/sprites-pipeline.yml instead.
# Enabling it here is harmless on sprites.
sudo systemctl enable --now pocketsmith-pipeline.timer

echo "==> Done. serve on :$PORT, pipeline timer armed."
systemctl --no-pager status pocketsmith-serve.service || true
