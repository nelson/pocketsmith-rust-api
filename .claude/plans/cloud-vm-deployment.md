# Deploy pocketsmith-sync to a public cloud VM (exe.dev / boxd.sh / sprites.dev)

## Status (implementation)

**Chosen vendor: sprites.dev.** All build/authoring work is complete and pushed
to branch `deploy/cloud-vms` → PR #41. Runtime verification (live VM, merged
release) is still pending — see the Verification checklist at the bottom.

Done:

- [x] `SERVE_HOST` patch in `src/bin/serve/main.rs` (compiles with `--features web`).
- [x] `.github/workflows/release.yml` — release-please + static-musl tarball build.
- [x] `release-please-config.json` + `.release-please-manifest.json` (pinned 0.1.0).
- [x] `deploy/install.sh` (owner=nelson), `deploy/systemd/*`, `deploy/provision-*.sh`.
- [x] `.github/workflows/sprites-pipeline.yml` — external scheduler, **sync-only** daily.
- [x] `deploy/README.md` — setup + how to switch vendors.
- [x] Branch pushed, PR #41 opened.

Deviations from the plan as written below:

- Owner placeholder `<owner>` resolved to `nelson`; branch is `deploy/cloud-vms`.
- The sprites scheduler lives at `.github/workflows/sprites-pipeline.yml` (the
  `deploy/.github/...` template copy in §6 was removed to avoid duplication).
- The daily sprites job runs `sync` only; the rest of the pipeline is commented
  out in the workflow.


## Context

Ship `pocketsmith-sync` to a public, low-maintenance cloud VM. The user wants to compare three SSH-native microVM providers and pick based on cost. This plan builds **deploy scripts for all three** so the choice is data-driven.

Facts driving the design:

- The crate is **pure Rust + bundled SQLite + rustls** → compiles to a single **static `x86_64-unknown-linux-musl` binary** with zero system deps.
- One crate produces 7 binaries (`serve`, `sync`, `transfers`, `normalise`, `categorise`, `push`, …); all config is env-driven (`SERVE_HOST`*, `SERVE_PORT`, `POCKETSMITH_DB`, `POCKETSMITH_API_KEY`, `POCKETSMITH_RULES_DIR`).
- `pocketsmith.db` holds review decisions = **durable state** → must live on the VM's persistent disk, never in the artifact.
- All three providers are **full Ubuntu microVMs you SSH into** — not a PaaS. So the universal artifact is the static binary, not a Docker image.

\* `SERVE_HOST` does not exist yet — see patch §1.

---

## Vendor landscape (researched)

| | **exe.dev** | **boxd.sh** | **sprites.dev** (Fly.io) |
|---|---|---|---|
| VM tech | Cloud Hypervisor, Ubuntu-derived ("exeuntu") | KVM microVM, Ubuntu 24.04 | Firecracker microVM, Ubuntu 24.04 |
| Create VM | `ssh exe.dev new --name X` | `ssh boxd.sh new --name=X` | `sprites` CLI / API (`POST /v1/sprites`) |
| HTTPS front door | `X.exe.xyz`, TLS + login auth, forwards to smallest exposed port | `X.boxd.sh`, TLS, forwards to **port 8000** (configurable via `boxd proxy set-port`) | unique URL, forwards to **port 8080**, public or authed |
| Always running? | VM stays up; **idle → $0 CPU**. Internal systemd timers **fire**. | Always-on, unlimited runtime. Timers **fire**. | **Scale-to-zero**: sleeps after idle, wakes on HTTP/command in <1s. **Internal cron does NOT fire while asleep.** |
| Docker | native (`--image`) but optional; can also run a binary | pre-installed; optional | possible but scale-to-zero favours a lean binary |
| Pricing | Usage: CPU $0.05/core·hr, RAM $0.016/GiB·hr, disk $0.08/GiB·mo (idle = $0 CPU) **or** Personal flat **$20/mo** | Individual flat **€20/mo** (verify free tier on live page); usage-based for teams; idle cost zero | Pure usage: CPU **$0.07**/CPU·hr, RAM **$0.04375**/GB·hr, storage **$0.000027**/GB·hr (cold) / $0.000683 (hot). $30 trial credit. No idle charge. |

**Critical design consequence:** on exe.dev and boxd the VM is alive 24/7, so a normal **systemd timer** runs the nightly pipeline. On sprites the VM sleeps, so an **external trigger** (a scheduled GitHub Actions job that hits the Sprite URL / runs the pipeline over SSH) is required — an in-VM cron would never wake it.

---

## Recommended artifact — static musl binary on GitHub Releases (not Docker)

Because every provider is a real Ubuntu VM, the simplest cross-vendor unit is a **static binary tarball published to GitHub Releases**:

- **No registry, no signup** — GitHub Releases are part of your repo (answers "do I need to register on ghcr.io?": with this path, *no registry at all*).
- Identical install on all three VMs: `curl` the tarball → drop binaries in `/usr/local/bin` → systemd.
- Tiny, instant cold-start (matters for sprites), no Docker daemon to run/pay for.

Docker stays available as an *optional* exe.dev-native path, but it is no longer the primary plan.

---

## Branch

```
git checkout -b deploy/cloud-vms
```

---

## Files to create / modify

### 1. `src/bin/serve/main.rs` — configurable bind host (REQUIRED, all vendors)

All three reach the server over the VM's network interface, so the hard-coded `127.0.0.1` is unreachable. Keep loopback as the safe local default.

```rust
// replace:
    let addr = format!("127.0.0.1:{port}");
// with:
    let host = std::env::var("SERVE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{host}:{port}");
```

### 2. `.github/workflows/release.yml` — auto-release on merge + build static binary

Answers "can I auto-cut a release on each merge?": **yes** — `release-please` watches merges to `master`, maintains a release PR, and when you merge that PR it tags + creates the GitHub Release. A second job then cross-compiles the musl binary and uploads the tarball asset. No manual tagging.

```yaml
name: Release
on:
  push:
    branches: [master]
permissions:
  contents: write
  pull-requests: write
jobs:
  release-please:
    runs-on: ubuntu-latest
    outputs:
      release_created: ${{ steps.rp.outputs.release_created }}
      tag_name: ${{ steps.rp.outputs.tag_name }}
    steps:
      - uses: googleapis/release-please-action@v4
        id: rp
        with:
          release-type: rust

  build-binary:
    needs: release-please
    if: ${{ needs.release-please.outputs.release_created }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: x86_64-unknown-linux-musl }
      - run: sudo apt-get update && sudo apt-get install -y musl-tools
      - run: cargo build --release --features web --target x86_64-unknown-linux-musl
              --bin serve --bin sync --bin transfers --bin normalise --bin categorise --bin push
      - name: Package
        run: |
          mkdir -p dist/rules && cp -r rules/* dist/rules/
          cp target/x86_64-unknown-linux-musl/release/{serve,sync,transfers,normalise,categorise,push} dist/
          tar -C dist -czf pocketsmith-sync-x86_64-linux-musl.tar.gz .
      - uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ needs.release-please.outputs.tag_name }}
          files: pocketsmith-sync-x86_64-linux-musl.tar.gz
```

> `release-please` derives versions from conventional-commit messages (`feat:` → minor, `fix:` → patch). If you don't want conventional commits, swap it for a "tag patch on every merge" step — noted as a fallback during implementation. The existing `ci.yml` (build+test on every push) stays unchanged.

### 3. `deploy/install.sh` — vendor-agnostic in-VM bootstrap

Run **on the VM**. Downloads the latest release tarball, installs binaries, writes systemd units, starts `serve`. Same script on every provider; only `SERVE_PORT` differs.

```bash
#!/usr/bin/env bash
set -euo pipefail
REPO="<owner>/pocketsmith-rust-api"     # set me
PORT="${SERVE_PORT:-3141}"              # exe.dev:3141  boxd:8000  sprites:8080
sudo mkdir -p /data /opt/pocketsmith
curl -fsSL "https://github.com/$REPO/releases/latest/download/pocketsmith-sync-x86_64-linux-musl.tar.gz" \
  | sudo tar -xz -C /opt/pocketsmith
sudo install /opt/pocketsmith/{serve,sync,transfers,normalise,categorise,push} /usr/local/bin/
# /data/pocketsmith.env must already contain POCKETSMITH_API_KEY (+ optional GOOGLE_PLACES_API_KEY)
cat <<EOF | sudo tee /etc/default/pocketsmith
POCKETSMITH_DB=/data/pocketsmith.db
POCKETSMITH_RULES_DIR=/opt/pocketsmith/rules
SERVE_HOST=0.0.0.0
SERVE_PORT=$PORT
EOF
sudo cp deploy/systemd/* /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pocketsmith-serve.service
sudo systemctl enable --now pocketsmith-pipeline.timer   # harmless on sprites; see §6
```

### 4. `deploy/systemd/` — serve unit + nightly pipeline (exe.dev & boxd)

`pocketsmith-serve.service`:
```ini
[Unit]
Description=PocketSmith web UI
After=network.target
[Service]
EnvironmentFile=/etc/default/pocketsmith
EnvironmentFile=/data/pocketsmith.env
ExecStart=/usr/local/bin/serve
Restart=always
[Install]
WantedBy=multi-user.target
```

`pocketsmith-pipeline.service` (oneshot chain) + `pocketsmith-pipeline.timer`:
```ini
# .service
[Service]
Type=oneshot
EnvironmentFile=/etc/default/pocketsmith
EnvironmentFile=/data/pocketsmith.env
ExecStartPre=/usr/local/bin/serve --version
ExecStart=/usr/local/bin/sync
ExecStart=/usr/local/bin/transfers
ExecStart=/usr/local/bin/transfers --apply
ExecStart=/usr/local/bin/categorise
ExecStart=/usr/local/bin/push
# .timer
[Timer]
OnCalendar=*-*-* 03:30:00
Persistent=true
[Install]
WantedBy=timers.target
```
(Add an `ExecStartPre` curl step if you also want pull-latest-binary before each run; confirm exact subcommand flags against `README.md` when wiring.)

### 5. Per-vendor provision wrappers (run from your laptop)

`deploy/provision-exe.sh`:
```bash
ssh exe.dev new --name pocketsmith --disk 10
ssh pocketsmith.exe.xyz 'printf "POCKETSMITH_API_KEY=%s\n" "$PS_KEY" | sudo tee /data/pocketsmith.env'
scp -r deploy pocketsmith.exe.xyz:~/ && ssh pocketsmith.exe.xyz 'SERVE_PORT=3141 bash deploy/install.sh'
```

`deploy/provision-boxd.sh`:
```bash
ssh boxd.sh new --name=pocketsmith
ssh pocketsmith.boxd.sh 'printf "POCKETSMITH_API_KEY=%s\n" "$PS_KEY" | sudo tee /data/pocketsmith.env'
scp -r deploy pocketsmith.boxd.sh:~/ && ssh pocketsmith.boxd.sh 'SERVE_PORT=8000 bash deploy/install.sh'
# (or keep 3141 and run: boxd proxy set-port --vm=pocketsmith --port=3141)
```

`deploy/provision-sprites.sh` — create Sprite, install with `SERVE_PORT=8080`, then rely on §6 for scheduling (CLI/API specifics confirmed at implementation time against docs.sprites.dev).

### 6. `deploy/.github/workflows/sprites-pipeline.yml` — external scheduler (sprites ONLY)

Because a sleeping Sprite won't run in-VM cron, an external scheduled workflow wakes it and runs the pipeline:
```yaml
name: Sprites nightly pipeline
on:
  schedule: [{ cron: "30 17 * * *" }]   # 03:30 Australia/Sydney ≈ 17:30 UTC
  workflow_dispatch:
jobs:
  run:
    runs-on: ubuntu-latest
    steps:
      - run: |   # SSH (or sprites CLI / curl the URL) into the Sprite and run the chain
          ssh pocketsmith.sprites '/usr/local/bin/sync && /usr/local/bin/transfers && \
            /usr/local/bin/transfers --apply && /usr/local/bin/categorise && /usr/local/bin/push'
```
(exe.dev/boxd ignore this file; they use the in-VM timer from §4.)

---

## What you execute on your end

**A. One time — branch + files:**
```
git checkout -b deploy/cloud-vms
# add files §1–§6, set <owner>
git add -A && git commit -m "feat: add cloud-VM deploy scripts (exe.dev/boxd/sprites)"
git push -u origin deploy/cloud-vms
```

**B. Releases — fully automatic.** Merge PRs to `master` with `feat:`/`fix:` messages → release-please opens a release PR → merge it → tag + GitHub Release + binary tarball are produced. No manual tagging or registry login.

**C. Provision (pick the vendor(s) you're trialling), with your API key exported:**
```
export PS_KEY=...your PocketSmith key...
bash deploy/provision-exe.sh       # → https://pocketsmith.exe.xyz
bash deploy/provision-boxd.sh      # → https://pocketsmith.boxd.sh
bash deploy/provision-sprites.sh   # → your Sprite URL
```

**D. Seed the DB once** (the live UI needs history): either `ssh <vm> /usr/local/bin/sync`, or `scp pocketsmith.db <vm>:/data/pocketsmith.db` before first start.

**E. Scheduling:** exe.dev & boxd — the timer is already enabled by `install.sh`. sprites — enable the GitHub Actions workflow in §6 (and add the Sprite's SSH details as repo secrets).

---

## Running costs & recommendation

Workload profile: idle ~23 hrs/day, a few-minute nightly pipeline, occasional manual review in the web UI; DB ~20 MB → a few GB of disk. Estimates for **one VM**:

| Provider | Model | Est. monthly cost | Notes |
|---|---|---|---|
| **exe.dev** (usage) | CPU/RAM/disk metered, idle = $0 CPU | **~$2–4** | VM stays alive → in-VM timer works; pay mostly for disk + brief bursts. Set a spend cap. |
| **exe.dev** (Personal flat) | $20/mo bundle | **$20** | Only worth it if you run many VMs. |
| **sprites.dev** (usage) | scale-to-zero, no idle charge | **~$1–4** | Cheapest while idle; but needs the **external** scheduler (§6). Cold wake <1s. |
| **boxd.sh** (Individual) | flat €20/mo (verify free tier) | **~$22** (or $0 if free tier applies) | Always-on, simplest scheduling; check live pricing for a free/cheaper tier. |

**Recommendation:** for this tiny, mostly-idle personal app, the **usage-based options (exe.dev usage or sprites.dev) are ~5× cheaper** than the flat plans, landing around **$2–4/mo**. Trade-off:

- Pick **exe.dev (usage)** for the simplest mental model — VM stays alive so the in-VM systemd timer "just works," and idle CPU is free.
- Pick **sprites.dev** for the lowest idle cost, accepting the external scheduler for the nightly job.
- Pick **boxd.sh** only if its flat €20 (or free tier) and always-on simplicity beat the metered options for you.

Since the scripts are built for all three, deploy to two and watch the first month's `billing usage` before settling. Avoid any constant registry-poller / keep-alive — it defeats idle billing on exe.dev and prevents sprites from ever sleeping.

---

## Verification

Runtime checks — all pending until PR #41 is merged and a Sprite is provisioned.

- [~] `cargo build --release --features web --target x86_64-unknown-linux-musl` produces static binaries locally. *(Runs in CI on `ubuntu-latest`; can't cross-compile on this aarch64-darwin/nix machine. Native `--features web` build verified instead.)*
- [ ] A merged PR triggers release-please → GitHub Release with `pocketsmith-sync-x86_64-linux-musl.tar.gz` attached.
- [ ] `bash deploy/install.sh` on a fresh VM installs binaries + units; `systemctl status pocketsmith-serve` is active.
- [ ] Web UI loads at the Sprite URL (port 8080) behind its auth.
- [ ] sprites: the scheduled GH workflow wakes the Sprite and runs `sync` (sync-only for now).
- [ ] exe.dev/boxd *(only if you add them later)*: `systemctl list-timers pocketsmith-pipeline.timer` shows next run; manual `systemctl start pocketsmith-pipeline.service` runs the full chain (`journalctl -u pocketsmith-pipeline`).
- [ ] Make a review decision in the UI, restart `serve`, decision persists (DB on `/data`).
- [ ] After ~1 month, compare `billing usage`.
