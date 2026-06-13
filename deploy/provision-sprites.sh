#!/usr/bin/env bash
# Provision pocketsmith-sync on sprites.dev (Fly.io). Run from your laptop.
# Requires: PS_KEY exported, and the sprites CLI installed + authenticated.
#
# NOTE on scheduling: a Sprite scales to zero (sleeps when idle), so the in-VM
# systemd timer will NOT fire the nightly pipeline. Scheduling is handled
# externally by .github/workflows/sprites-pipeline.yml (a scheduled GitHub Actions
# job that wakes the Sprite over SSH and runs the chain). Enable that workflow
# and add the Sprite's SSH host as a repo secret after provisioning.
#
# Sprites forward their URL to port 8080. Confirm exact CLI flags against
# https://docs.sprites.dev (create + SSH/host commands) at run time.
set -euo pipefail
: "${PS_KEY:?export PS_KEY=your-pocketsmith-api-key first}"
VM="pocketsmith"

# Create the Sprite (adjust to the current sprites CLI; --url public/authed).
sprites create "$VM" || sprites new "$VM"
HOST="$(sprites ssh-host "$VM" 2>/dev/null || echo "$VM.sprites")"

ssh "$HOST" "sudo mkdir -p /data && printf 'POCKETSMITH_API_KEY=%s\n' '$PS_KEY' | sudo tee /data/pocketsmith.env >/dev/null"
scp -r deploy "$HOST:~/"
ssh "$HOST" "SERVE_PORT=8080 bash deploy/install.sh"
echo "==> Web UI: your Sprite URL (port 8080)"
echo "==> Scheduled runs use .github/workflows/sprites-pipeline.yml (add SPRITE_SSH_* secrets)."
