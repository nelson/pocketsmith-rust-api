#!/usr/bin/env bash
# Provision pocketsmith on exe.dev. Run from your laptop.
# Requires: PS_KEY exported (your PocketSmith API key).
set -euo pipefail
: "${PS_KEY:?export PS_KEY=your-pocketsmith-api-key first}"
VM="pocketsmith"
HOST="$VM.exe.xyz"

ssh exe.dev new --name "$VM" --disk 10
# exe.dev forwards the front door to the smallest exposed port; serve uses 3141.
ssh "$HOST" "sudo mkdir -p /data && printf 'POCKETSMITH_API_KEY=%s\n' '$PS_KEY' | sudo tee /data/pocketsmith.env >/dev/null"
scp -r deploy "$HOST:~/"
ssh "$HOST" "SERVE_PORT=3141 bash deploy/install.sh"
echo "==> Web UI: https://$HOST  (behind exe.dev login auth)"
