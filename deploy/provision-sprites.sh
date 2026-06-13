#!/usr/bin/env bash
# Provision pocketsmith-sync on sprites.dev using the `sprite` CLI.
#
# Prereqs:
#   - sprite CLI installed and authenticated:
#       sprite login                 # interactive, or
#       sprite auth setup --token "<org>/<id>/<token-id>/<token-value>"
#   - PS_KEY exported (your PocketSmith API key).
#
# Idempotent: if a sprite named "$VM" already exists it is REUSED, not
# recreated. Safe to re-run to re-provision / upgrade binaries.
#
# sprites.dev has no SSH, so everything runs through `sprite exec`. The sprite's
# public URL is proxied to port 8080, so `serve` listens there. The nightly
# `sync` is driven externally by .github/workflows/sprites-pipeline.yml (a
# sleeping sprite can't run its own cron).
set -euo pipefail
: "${PS_KEY:?export PS_KEY=your-pocketsmith-api-key first}"
VM="${VM:-pocketsmith}"
REPO="${REPO:-nelson/pocketsmith-rust-api}"
PORT=8080   # sprites proxy the public URL to port 8080

# Release assets live in a (private) repo, so pass a GitHub token for download.
GH_TOKEN="${GH_TOKEN:-$(gh auth token --hostname github.com 2>/dev/null || true)}"
AUTH_HEADER=""
[ -n "$GH_TOKEN" ] && AUTH_HEADER="-H \"Authorization: Bearer $GH_TOKEN\""

# 1. Create ONLY if it doesn't already exist (`sprite ls` lists sprites).
if sprite ls 2>/dev/null | grep -qw "$VM"; then
  echo "==> Sprite '$VM' already exists; reusing it (not creating)."
else
  echo "==> Creating sprite '$VM'."
  sprite create "$VM"
fi

# 2. Provision over `sprite exec` (reads this bootstrap script on stdin).
#    If your sprite CLI doesn't forward stdin to exec, switch to
#    `sprite -s "$VM" exec -- bash -c '<inline>'`.
echo "==> Provisioning '$VM'..."
sprite -s "$VM" exec -- bash -se <<REMOTE
set -euo pipefail
sudo mkdir -p /data /opt/pocketsmith
printf 'POCKETSMITH_API_KEY=%s\n' '$PS_KEY' | sudo tee /data/pocketsmith.env >/dev/null

curl -fsSL $AUTH_HEADER \
  "https://github.com/$REPO/releases/latest/download/pocketsmith-sync-x86_64-linux-musl.tar.gz" \
  | sudo tar -xz -C /opt/pocketsmith

# Install whichever binaries the release contains (categorise may be absent
# until that work merges).
for b in serve sync transfers normalise categorise push; do
  [ -f "/opt/pocketsmith/\$b" ] && sudo install "/opt/pocketsmith/\$b" "/usr/local/bin/\$b" || true
done

sudo tee /etc/systemd/system/pocketsmith-serve.service >/dev/null <<UNIT
[Unit]
Description=PocketSmith web UI
After=network.target
[Service]
Environment=POCKETSMITH_DB=/data/pocketsmith.db
Environment=POCKETSMITH_RULES_DIR=/opt/pocketsmith/rules
Environment=SERVE_HOST=0.0.0.0
Environment=SERVE_PORT=$PORT
EnvironmentFile=/data/pocketsmith.env
ExecStart=/usr/local/bin/serve
Restart=always
[Install]
WantedBy=multi-user.target
UNIT

sudo systemctl daemon-reload
sudo systemctl enable --now pocketsmith-serve.service
echo "serve installed and started on port $PORT"
REMOTE

echo "==> Web UI URL:"
sprite -s "$VM" url || true
echo "==> Seed the DB once:  sprite -s $VM exec -- /usr/local/bin/sync"
echo "==> Nightly sync: set repo secret SPRITE_TOKEN; see .github/workflows/sprites-pipeline.yml"
