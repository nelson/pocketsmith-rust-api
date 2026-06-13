#!/usr/bin/env bash
# Provision pocketsmith-sync on boxd.sh. Run from your laptop.
# Requires: PS_KEY exported (your PocketSmith API key).
set -euo pipefail
: "${PS_KEY:?export PS_KEY=your-pocketsmith-api-key first}"
VM="pocketsmith"
HOST="$VM.boxd.sh"

ssh boxd.sh new --name="$VM"
ssh "$HOST" "sudo mkdir -p /data && printf 'POCKETSMITH_API_KEY=%s\n' '$PS_KEY' | sudo tee /data/pocketsmith.env >/dev/null"
scp -r deploy "$HOST:~/"
# boxd's default proxy forwards name.boxd.sh -> port 8000, so run serve there.
# (Alternative: keep 3141 and remap with `boxd proxy set-port --vm=$VM --port=3141`.)
ssh "$HOST" "SERVE_PORT=8000 bash deploy/install.sh"
echo "==> Web UI: https://$HOST"
