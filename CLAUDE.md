# CLAUDE.md — workspace notes for pi sessions

## Toolchain paths

The pi sandbox on this machine blocks `/run/current-system/sw/bin/` (returns
"Operation not permitted" on `stat`/`exec`/`ls`). Use direct nix-store paths
for Rust tooling instead:

```
CARGO=/nix/store/v21x5yl04z0l303iz6ir5aqy9jzzrn76-cargo-1.93.0/bin/cargo
```

Verify with `$CARGO --version`. If the store path has been GC'd, search
`/nix/store/` for `^[a-z0-9]+-cargo-1\.` and pick the highest version.

`rustc` ships alongside under the same store derivation pattern; `cargo`
invokes it via PATH-relative lookup so no extra wiring needed.

## Project

`pocketsmith-rust-api` — local mirror + sync for Pocketsmith.

Binaries (see `Cargo.toml`):
- `sync` — pull from Pocketsmith API
- `transfers` — detect/apply transfer pairs (scan/apply paradigm)
- `normalise` — payee normalisation (scan/apply paradigm)
- `push` — push local edits upstream
- `serve` — review UI (requires `--features web`)
