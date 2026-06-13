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
# IMPORTANT: sprites.dev has NO systemd. It runs its own service manager
# (`sprite-env services`, executed inside the sprite). Services auto-restart
# when the sprite wakes from sleep; processes started via `exec`/`console` do
# NOT survive sleep. The sprite's public URL is proxied to the service's
# --http-port (8080 here). serve + sync auto-load /data/.env via dotenv when
# run with cwd=/data, so we keep a single env file there.
set -euo pipefail
: "${PS_KEY:?export PS_KEY=your-pocketsmith-api-key first}"
VM="${VM:-pocketsmith}"
REPO="${REPO:-nelson/pocketsmith-rust-api}"
PORT=8080   # sprites proxy the public URL to this port

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

# Single env file, auto-loaded by serve and sync (dotenv) when cwd=/data.
sudo tee /data/.env >/dev/null <<ENVF
POCKETSMITH_API_KEY=$PS_KEY
POCKETSMITH_DB=/data/pocketsmith.db
POCKETSMITH_RULES_DIR=/opt/pocketsmith/rules
SERVE_HOST=0.0.0.0
SERVE_PORT=$PORT
ENVF

curl -fsSL $AUTH_HEADER \
  "https://github.com/$REPO/releases/latest/download/pocketsmith-sync-x86_64-linux-musl.tar.gz" \
  | sudo tar -xz -C /opt/pocketsmith

# Install whichever binaries the release contains (categorise may be absent
# until that work merges).
for b in serve sync transfers normalise categorise push; do
  [ -f "/opt/pocketsmith/\$b" ] && sudo install "/opt/pocketsmith/\$b" "/usr/local/bin/\$b" || true
done

# Register serve as a Sprite service (auto-restarts on wake, proxied to $PORT).
# Wrapped in 'bash -c cd /data && exec serve' so dotenv finds /data/.env.
# Idempotent: drop any prior definition first.
sprite-env services delete pocketsmith-serve >/dev/null 2>&1 || true
sprite-env services create pocketsmith-serve \
  --cmd bash --args '-c,cd /data && exec /usr/local/bin/serve' \
  --http-port $PORT
echo "serve registered as a sprite service on port $PORT"
REMOTE

echo "==> Seeding the database (first sync)..."
sprite -s "$VM" exec -- bash -c 'cd /data && sudo -E /usr/local/bin/sync' || \
  echo "    (seed skipped/failed; run manually: sprite -s $VM exec -- bash -c 'cd /data && sudo -E /usr/local/bin/sync')"

echo "==> Web UI URL (first hit wakes the sprite + starts the service):"
sprite -s "$VM" url || true
echo "==> Nightly sync: set repo secret SPRITE_TOKEN; see .github/workflows/sprites-pipeline.yml"
