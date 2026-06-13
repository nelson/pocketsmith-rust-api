#!/usr/bin/env bash
# Install / upgrade pocketsmith-sync on sprites.dev using the `sprite` CLI.
#
# This is the deploy entrypoint for sprites. It is idempotent and serves both
# the first install and every later upgrade:
#   - first run: creates the sprite (if absent), writes /data/.env, installs
#     binaries, registers the `serve` service, seeds the DB.
#   - later runs: reuses the sprite, re-downloads releases/latest + reinstalls
#     binaries, recreates the service (picks up the new binary), and SKIPS the
#     env write and DB seed if they already exist. Never wipes /data.
#
# Prereqs:
#   - sprite CLI installed and authenticated (`sprite login`, or
#     `sprite auth setup --token "<org>/<id>/<token-id>/<token-value>"`).
#   - PS_KEY exported on FIRST install only (your PocketSmith API key). Upgrades
#     don't need it (the key is already in /data/.env), so this can run headless
#     in CI with just sprite auth + a GitHub token for the asset download.
#
# sprites.dev has NO systemd; it runs its own service manager (`sprite-env
# services`, executed inside the sprite). Services auto-restart on wake; the
# URL proxy routes to --http-port and starts the service on incoming requests.
# serve + sync auto-load /data/.env via dotenv when run with cwd=/data.
set -euo pipefail
VM="${VM:-pocketsmith}"
REPO="${REPO:-nelson/pocketsmith-rust-api}"
PORT=8080   # sprites proxy the public URL to this port
PS_KEY="${PS_KEY:-}"   # required on first install only

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

# 2. Install/upgrade over `sprite exec` (reads this bootstrap on stdin).
echo "==> Installing/upgrading on '$VM'..."
sprite -s "$VM" exec -- bash -se <<REMOTE
set -euo pipefail
sudo mkdir -p /data /opt/pocketsmith

# Env file is written on first install only (preserves your key on upgrades).
if [ ! -f /data/.env ]; then
  if [ -z "$PS_KEY" ]; then
    echo "ERROR: first install needs PS_KEY exported (PocketSmith API key)" >&2
    exit 1
  fi
  printf '%s\n' "POCKETSMITH_API_KEY=$PS_KEY" "POCKETSMITH_DB=/data/pocketsmith.db" "POCKETSMITH_RULES_DIR=/opt/pocketsmith/rules" "SERVE_HOST=0.0.0.0" "SERVE_PORT=$PORT" | sudo tee /data/.env >/dev/null
  echo "wrote /data/.env"
else
  echo "/data/.env exists; keeping existing config"
fi

# Download + install whichever binaries the release contains (single line: no
# fragile backslash-continuations inside the heredoc).
curl -fsSL $AUTH_HEADER "https://github.com/$REPO/releases/latest/download/pocketsmith-sync-x86_64-linux-musl.tar.gz" | sudo tar -xz -C /opt/pocketsmith
for b in serve sync transfers normalise categorise push; do
  [ -f "/opt/pocketsmith/\$b" ] && sudo install "/opt/pocketsmith/\$b" "/usr/local/bin/\$b" || true
done

# Register serve as a Sprite service (single line). --dir /data => cwd=/data so
# serve auto-loads /data/.env. --no-stream returns immediately. Only one service
# may own the HTTP port, so drop any prior definition first.
sprite-env services delete pocketsmith-serve >/dev/null 2>&1 || true
sprite-env services create pocketsmith-serve --cmd /usr/local/bin/serve --dir /data --http-port $PORT --no-stream
echo "serve registered as a sprite service on port $PORT"
REMOTE

# 3. Seed the DB on FIRST install only (skip if it already exists; the nightly
#    workflow keeps it fresh thereafter). Single-line remote command.
echo "==> Seeding the database (first run only)..."
sprite -s "$VM" exec -- bash -c 'if [ -f /data/pocketsmith.db ]; then echo "    /data/pocketsmith.db exists — skipping seed."; else cd /data && sudo -E /usr/local/bin/sync; fi' || \
  echo "    (seed skipped/failed; seed manually later)"

echo "==> Web UI URL (first hit wakes the sprite + starts the service):"
sprite -s "$VM" url || true
echo "==> Nightly sync: set repo secret SPRITE_TOKEN; see .github/workflows/sprites-pipeline.yml"
