# Plan: Consolidate binaries into `pocketsmith-rust-api` + XDG DB + version/justfile

## Context

The tool maintains a long-lived SQLite database synced daily. Today it ships as
**seven separate binaries** (`sync`, `transfers`, `normalise`, `serve`, `push`,
`dump`, `rule`) and defaults the DB to `./pocketsmith.db` in the cwd. We want to:

1. **Move the default DB path** out of the cwd to the user data dir
   (`$XDG_DATA_HOME`, falling back to `~/.local/share`), so a long-lived DB isn't
   tied to a working directory.
2. **Consolidate every binary into a single `pocketsmith-rust-api` binary** whose
   first argument is a subcommand (`sync`, `transfers`, … plus new `version` and
   `help`).
3. Add a **`version`** subcommand that prints the current release version.
4. Add a **justfile** with `build`, `install`, `release`.
5. Print the version when running **`help`** and when invoking the binary with
   **no command**.

The production pipeline (systemd, sprites) currently calls each binary by path
(`/usr/local/bin/sync`, …). Per the user's decision, those will move to
subcommand form (`pocketsmith-rust-api sync`, …) and deploy assets get updated.

## Approach

### Single binary with subcommand dispatch
- One `[[bin]] name = "pocketsmith-rust-api"  path = "src/main.rs"` in
  `Cargo.toml`; remove the other six `[[bin]]` entries.
- `src/main.rs` becomes a dispatcher: parse `argv[1]` as the subcommand, route to
  a per-command `run(args: &[String])`. Returns `std::process::ExitCode`.
- Subcommand logic relocated from the old bins into binary-local modules under
  `src/`:
  - `src/cli/{sync,transfers,normalise,push,dump}.rs` — small bins, each old
    `fn main()` becomes `pub fn run(args)`.
  - `src/rule/` — move `src/bin/rule/*` here; `main.rs` → `mod.rs`, `fn main`
    → `pub fn run(args) -> ExitCode`; blanket-rewrite `crate::` → `crate::rule::`.
  - `src/serve/` — move `src/bin/serve/*` here; `main.rs` → `mod.rs`, `fn main`
    → `pub fn run() -> Result<()>`; blanket-rewrite `crate::` → `crate::serve::`.
    Gated `#[cfg(feature = "web")]` so the binary still builds without `web`;
    the `serve` arm errors with a helpful message when built without `web`.
- All old bins use only `pocketsmith_sync::…` (lib) for real logic, so moving the
  thin shells is mechanical. `crate::` inside serve/rule only ever refers to
  sibling bin modules, so the blanket prefix rewrite is safe.

### Commands the dispatcher handles
| argv | behaviour |
|------|-----------|
| (none) | print version banner, then help; exit 0 |
| `help` / `--help` / `-h` | print version banner, then help; exit 0 |
| `version` / `--version` / `-V` | print `pocketsmith-rust-api <version>`; exit 0 |
| `sync` | `cli::sync::run` |
| `transfers` | `cli::transfers::run` |
| `normalise` | `cli::normalise::run` |
| `push` | `cli::push::run` |
| `dump` | `cli::dump::run` |
| `rule …` | `rule::run` (its own sub-verbs/flags) |
| `serve` | `serve::run` (web feature) |
| other | error + help; exit 2 |

### Version source
`env!("CARGO_PKG_VERSION")` — release-please (`release-type: rust`) keeps
`Cargo.toml`'s `version` in lockstep with the most recent release tag and the
`.release-please-manifest.json`. So the compiled-in `CARGO_PKG_VERSION` *is* the
most recent release number. No extra plumbing needed.

### XDG default DB path
In `src/db/mod.rs`:
- `path_from_env()`: keep honouring `POCKETSMITH_DB` first. Otherwise return
  `$XDG_DATA_HOME/pocketsmith/pocketsmith.db`, falling back to
  `$HOME/.local/share/pocketsmith/pocketsmith.db` when `XDG_DATA_HOME` is unset.
  (Subdirectory name: **`pocketsmith`**.)
- `initialize()` must `std::fs::create_dir_all(parent)` before
  `Connection::open` so first run on a fresh machine works.
- Production is unaffected: systemd/sprites set `POCKETSMITH_DB=/data/pocketsmith.db`.

### justfile
```just
build:
    cargo build --release --features web

install: build
    cargo install --path . --features web   # installs `pocketsmith-rust-api`

# Releases are automated by release-please: pushing master updates/cuts the
# release PR, and merging it tags + builds the musl tarball in CI. `just
# release` just pushes the current branch so that pipeline runs.
release:
    git push origin HEAD
```
**Decision:** `just release` triggers the existing release-please/CI flow (push
to master) rather than building/tagging locally.

## Files to modify / create
- `Cargo.toml` — collapse `[[bin]]` list to one; keep `web` feature.
- `src/main.rs` — dispatcher (replaces current sync-only main).
- `src/cli/mod.rs` + `src/cli/{sync,transfers,normalise,push,dump}.rs` — new.
- `src/rule/` — moved from `src/bin/rule/` (mod.rs, crate:: rewrite).
- `src/serve/` — moved from `src/bin/serve/` (mod.rs, crate:: rewrite, web-gated).
- Delete `src/bin/` once emptied (so Cargo doesn't auto-discover stray bins).
- `src/db/mod.rs` — XDG path + create_dir_all.
- `justfile` — new.
- `tests/rule_cli.rs` — use `CARGO_BIN_EXE_pocketsmith-rust-api` and prepend
  `"rule"` to args.
- Deploy: `deploy/systemd/pocketsmith-pipeline.service`,
  `deploy/systemd/pocketsmith-serve.service`, `deploy/install.sh`,
  `deploy/deploy-sprites.sh`, `.github/workflows/sprites-pipeline.yml`,
  `deploy/README.md`, `README.md` — switch to `pocketsmith-rust-api <cmd>` and
  install a single binary.

## Reuse (existing code, no rewrites)
- All real logic stays in the library: `pocketsmith_sync::{sync,transfers,
  normalise,push,rules,db,client}`.
- `db::open_app_db` / `open_app_db_at` / `path_from_env` already centralise DB
  opening for every command — only the default-path branch changes.
- `serve`/`rule` internal module trees move verbatim aside from the `crate::`
  prefix and `main`→`run` rename.

## Steps
- [ ] Add XDG default path + `create_dir_all` in `src/db/mod.rs`; keep
      `POCKETSMITH_DB` override.
- [ ] Create `src/cli/` modules from `transfers.rs`, `normalise.rs`, `push.rs`,
      `dump.rs`, and current `main.rs` sync logic (`fn main`→`pub fn run`).
- [ ] Move `src/bin/rule/` → `src/rule/`; `main.rs`→`mod.rs`; rewrite
      `crate::`→`crate::rule::`; `fn main`→`pub fn run(args)->ExitCode`.
- [ ] Move `src/bin/serve/` → `src/serve/`; `main.rs`→`mod.rs`; rewrite
      `crate::`→`crate::serve::`; `fn main`→`pub fn run()->Result<()>`; gate
      `#[cfg(feature="web")]`.
- [ ] Rewrite `src/main.rs` as the dispatcher (version/help/no-arg + routing).
- [ ] Collapse `[[bin]]` entries in `Cargo.toml` to the single binary.
- [ ] Delete emptied `src/bin/`.
- [ ] Add `justfile` (build/install/release).
- [ ] Update `tests/rule_cli.rs` to the unified binary + `rule` prefix.
- [ ] Update deploy/systemd/CI/docs to subcommand form + single-binary install.

## Verification
- [ ] `cargo build --features web` and `cargo build` (no web) both succeed under
      `warnings = "deny"`.
- [ ] `cargo test --features web` passes (incl. serve smoke/pipeline tests and
      updated `tests/rule_cli.rs`).
- [ ] `cargo run -- version` prints the Cargo.toml version.
- [ ] `cargo run --` (no args) and `cargo run -- help` both print the version.
- [ ] `cargo run -- rule list --stage merchants` works (sub-verb routing).
- [ ] `cargo run --features web -- serve` boots the web UI.
- [ ] With `POCKETSMITH_DB` unset, `cargo run -- sync` creates the DB under
      `$XDG_DATA_HOME/pocketsmith/` (dir auto-created); with it set, uses that path.
- [ ] `just build` / `just install` / `just release` run.

## Resolved decisions
1. `just release` = `git push origin HEAD` to trigger release-please/CI.
2. XDG subdirectory name = `pocketsmith` (`$XDG_DATA_HOME/pocketsmith/pocketsmith.db`).
