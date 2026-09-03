#!/usr/bin/env bash
# Install / upgrade pocketsmith on sprites.dev using the `sprite` CLI.
#
# This is the deploy entrypoint for sprites. It is idempotent and serves both
# the first install and every later upgrade:
#   - first run: creates the sprite (if absent), writes /data/.env, installs
#     binaries, registers the `serve` service, seeds the DB.
#   - later runs: reuses the sprite, re-downloads releases/latest + reinstalls
#     binaries, recreates the service (picks up the new binary), and SKIPS the
#     env write + DB seed if they already exist. Never wipes /data.
#
# Prereqs:
#   - sprite CLI installed and authenticated (`sprite login`, or
#     `sprite auth setup --token "<org>/<id>/<token-id>/<token-value>"`).
#   - PS_KEY exported on FIRST install only (your PocketSmith API key). Upgrades
#     don't need it (the key is already in /data/.env), so this can run headless
#     in CI with just Sprite auth + a GitHub token for the asset download.
#
# sprites.dev has NO systemd; it runs its own service manager (`sprite-env
# services`, executed inside the Sprite). Services auto-restart on wake; the
# URL proxy routes to --http-port and starts the service on incoming requests.
# serve + sync auto-load /data/.env via dotenv when run with cwd=/data.
set -euo pipefail
VM="${VM:-pocketsmith}"
REPO="${REPO:-nelson/pocketsmith-rust-api}"
PORT=8080
PS_KEY="${PS_KEY:-}"
REPORTING_API_TOKEN="${REPORTING_API_TOKEN:-}"
REPORTING_API_TOKEN_B64="$(printf '%s' "$REPORTING_API_TOKEN" | base64 | tr -d '\n')"

# Pass a GitHub token when available so this also works if the repo becomes
# private later. Public release downloads work without one.
GH_TOKEN="${GH_TOKEN:-$(gh auth token --hostname github.com 2>/dev/null || true)}"

# 1. Create ONLY if it doesn't already exist (`sprite list` lists Sprites).
if sprite list 2>/dev/null | grep -qw "$VM"; then
  echo "==> Sprite '$VM' already exists; reusing it (not creating)."
else
  echo "==> Creating Sprite '$VM'."
  sprite create "$VM"
fi

# 2. Install/upgrade over `sprite exec` (reads this bootstrap on stdin).
echo "==> Installing/upgrading on '$VM'..."
sprite exec -s "$VM" -- bash -se <<REMOTE
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

# Enable the externally callable surface only when a separate, least-privilege
# reporting credential has been configured. The Sprite URL remains private
# until an operator explicitly changes its auth mode.
if [ -n "$REPORTING_API_TOKEN_B64" ]; then
  reporting_token="\$(printf '%s' "$REPORTING_API_TOKEN_B64" | base64 -d)"
  sudo awk '!/^REPORTING_API_TOKEN=|^SERVE_API_ONLY=/' /data/.env > /tmp/pocketsmith.env
  printf '%s\n' "REPORTING_API_TOKEN=\$reporting_token" "SERVE_API_ONLY=1" | sudo tee -a /tmp/pocketsmith.env >/dev/null
  sudo install -m 0600 /tmp/pocketsmith.env /data/.env
  sudo rm -f /tmp/pocketsmith.env
  echo "configured token-protected read-only reporting mode"
fi

# Download + install the single `pocketsmith` binary.
if [ -n "$GH_TOKEN" ]; then
  curl -fsSL -H "Authorization: Bearer $GH_TOKEN" "https://github.com/$REPO/releases/latest/download/pocketsmith-x86_64-linux-musl.tar.gz" | sudo tar -xz -C /opt/pocketsmith
else
  curl -fsSL "https://github.com/$REPO/releases/latest/download/pocketsmith-x86_64-linux-musl.tar.gz" | sudo tar -xz -C /opt/pocketsmith
fi
sudo install /opt/pocketsmith/pocketsmith /usr/local/bin/pocketsmith

# Register serve as a Sprite service. --dir /data makes dotenv load /data/.env.
sprite-env services delete pocketsmith-serve >/dev/null 2>&1 || true
sprite-env services create pocketsmith-serve --cmd /usr/local/bin/pocketsmith --args serve --dir /data --http-port $PORT --no-stream
echo "serve registered as a Sprite service on port $PORT"
REMOTE

# 3. Seed the DB on FIRST install only. The scheduled workflow keeps it fresh.
echo "==> Seeding the database (first run only)..."
sprite exec -s "$VM" -- bash -c 'if [ -f /data/pocketsmith.db ]; then echo "    /data/pocketsmith.db exists — skipping seed."; else cd /data && sudo -E /usr/local/bin/pocketsmith sync; fi' || \
  echo "    (seed skipped/failed; seed manually later)"

echo "==> Web UI URL (first hit wakes the Sprite + starts the service):"
sprite url -s "$VM" || true
echo "==> Scheduled sync: set repo secret SPRITE_TOKEN; see .github/workflows/sprites-pipeline.yml"
