# Deploying pocketsmith-sync

Zero-setup deployment to SSH-native microVM providers. The build artifact is a
single **static `x86_64-unknown-linux-musl` binary tarball** published to GitHub
Releases — no registry, no system dependencies on the VM.

**Currently configured for: [sprites.dev](https://sprites.dev).**

## How it fits together

| Piece | Role |
|---|---|
| `.github/workflows/release.yml` | On merge to `master`, release-please cuts a version + builds/attaches `pocketsmith-sync-x86_64-linux-musl.tar.gz` to the GitHub Release. |
| `install.sh` | Vendor-agnostic in-VM bootstrap: downloads the latest release, installs binaries + `rules/`, writes systemd units, starts `serve`. Only `SERVE_PORT` differs per vendor. |
| `systemd/pocketsmith-serve.service` | Runs the web UI. |
| `systemd/pocketsmith-pipeline.{service,timer}` | Nightly `sync → transfers → categorise → push`. **Fires only on always-on VMs** (exe.dev / boxd). |
| `provision-*.sh` (exe/boxd), `deploy-sprites.sh` | Per-vendor deploy entrypoints. sprites uses `deploy-sprites.sh` (install **and** upgrade); exe/boxd use `provision-*.sh` + `install.sh`. |
| `../.github/workflows/sprites-pipeline.yml` | **sprites only** — external nightly scheduler (a sleeping Sprite can't run an in-VM timer). Currently runs `sync` only. |

State (`pocketsmith.db`, the review decisions) lives on the VM's persistent disk
at `/data` and is never baked into the artifact.

## Deploying to sprites.dev (the configured vendor)

1. Merge a `feat:`/`fix:` PR to `master`, then merge the release-please PR it
   opens → first release + binary tarball.
2. **First install:** `export PS_KEY=<your-pocketsmith-key>` then
   `bash deploy/deploy-sprites.sh` (sprites CLI authed; `serve` on port **8080**).
   The same script is the **upgrade** path — re-run it (no `PS_KEY` needed) to
   pull a newer release; it reuses the sprite, reinstalls binaries, recreates
   the service, and skips the env write + DB seed.
3. Add repo secret `SPRITE_TOKEN` (a `sprite auth setup` token) so
   `.github/workflows/sprites-pipeline.yml` can wake the Sprite nightly **and**
   so releases auto-deploy (below).

**Continuous deploy:** once `SPRITE_TOKEN` is set, the `deploy-sprites` job in
`.github/workflows/release.yml` runs `deploy-sprites.sh` automatically on every
release (after the binary is attached), so the sprite always tracks the latest
release. It skips cleanly if the secret is absent. The first install stays
manual (it needs `PS_KEY`); CI only performs upgrades.

**sprites.dev has no systemd.** `deploy-sprites.sh` does *not* use
`install.sh`/`systemd/`; it talks to the sprite's own service manager
(`sprite-env services`) over the `sprite` CLI. serve is registered as a service
so it **auto-restarts whenever the sprite wakes** (processes started via
`sprite exec`/`console` don't survive sleep). serve + sync auto-load `/data/.env`
via dotenv (run with cwd `/data`).

Scheduling note: Sprites scale to zero, so an in-VM timer never fires. The
nightly job is driven externally by the GitHub Actions workflow, which is
intentionally `sync`-only right now (re-enable the rest of the chain in that
file when ready).

---

## Switching to / adding another vendor

The binary, `install.sh`, and the systemd units are vendor-neutral. Each vendor
differs in only three things: **how you create the VM**, **the front-door port**,
and **how the nightly pipeline is scheduled**.

### exe.dev

- **Create + install:** `export PS_KEY=...` then `bash deploy/provision-exe.sh`
  (front door forwards to the smallest exposed port; `serve` uses **3141**).
- **URL:** `https://pocketsmith.exe.xyz` (behind exe.dev login auth).
- **Scheduling:** the VM stays alive (idle CPU is free), so the **in-VM systemd
  timer works** — `install.sh` already enables `pocketsmith-pipeline.timer`.
  Nothing else to do.
- **Sprites workflow:** not needed. Disable it so it doesn't run pointlessly —
  delete `.github/workflows/sprites-pipeline.yml` or remove its `schedule:`
  trigger.

### boxd.sh

- **Create + install:** `export PS_KEY=...` then `bash deploy/provision-boxd.sh`.
  The default proxy forwards `name.boxd.sh → port 8000`, so `serve` runs on
  **8000**. (Alternative: keep 3141 and remap with
  `boxd proxy set-port --vm=pocketsmith --port=3141`.)
- **URL:** `https://pocketsmith.boxd.sh`.
- **Scheduling:** always-on VM → **in-VM systemd timer works**; nothing else to
  do.
- **Sprites workflow:** not needed — disable it as above.

### General checklist when changing vendors

1. Provision with the matching `provision-<vendor>.sh` (sets the right
   `SERVE_PORT`).
2. **Scheduling:** always-on vendor (exe.dev/boxd) → rely on the in-VM timer and
   **disable** `.github/workflows/sprites-pipeline.yml`. Scale-to-zero vendor
   (sprites) → keep the external GitHub Actions schedule and the
   `SPRITE_SSH_*` secrets.
3. Seed the DB once: `ssh <vm> /usr/local/bin/sync`, or
   `scp pocketsmith.db <vm>:/data/pocketsmith.db` before first start.
4. Adjust the nightly time: the in-VM timer uses local VM time
   (`OnCalendar` in `systemd/pocketsmith-pipeline.timer`, set the VM timezone);
   the sprites workflow uses a UTC cron.
