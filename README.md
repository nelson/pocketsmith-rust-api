# pocketsmith-sync

A Rust CLI for syncing PocketSmith data to a local SQLite database and normalising transaction payees via a rule-based pipeline.

## What it does

**`pocketsmith-sync`** (default binary) pulls your PocketSmith account data -- users, transaction accounts, categories, and transactions -- into a local `pocketsmith.db` SQLite file. It supports incremental sync using `updated_since` timestamps tracked in a change log.

**`normalise`** (secondary binary, in progress) runs transaction payees through a 5-stage normalisation pipeline:

1. **Strip** -- removes known prefixes (e.g. `SQ *`) and suffixes (e.g. `Card xx1234 Value Date ...`, `PTY LTD`). Repeats until stable.
2. **Classify** -- tags the transaction type (merchant, salary, transfer, banking operation, etc.) based on the original payee.
3. **Expand** -- expands truncated location names (e.g. `STRATHFIEL` to `STRATHFIELD`).
4. **Identify** -- matches known entities and normalises to a canonical name with optional location.
5. **Cleanup** -- final passes for title casing, whitespace, and formatting. Repeats until stable.

Rules for each stage are defined in YAML files under `rules/`.

## Prerequisites

- Rust toolchain (stable). Install via [rustup](https://rustup.rs/).
- A PocketSmith account with an API developer key. Generate one from PocketSmith under Settings > Developer.

## Installation

```
git clone <repo-url>
cd pocketsmith-rust-api
cargo build --release
```

Binaries are placed in `target/release/`.

## Configuration

Create a `.env` file in the project root:

```
POCKETSMITH_API_KEY=your_api_key_here
```

Or export the variable directly:

```
export POCKETSMITH_API_KEY=your_api_key_here
```

## Usage

### Sync

Pull all PocketSmith data into `pocketsmith.db`:

```
cargo run
```

On first run, it fetches all transactions. Subsequent runs fetch only transactions updated since the last sync.

The local database schema includes change tracking: a `_transaction_change_log` table records each sync, and `_transactions_history` captures field-level diffs (payee, category, note, labels, is_transfer, memo) via SQLite triggers.

### Normalise

```
cargo run --bin normalise
```

Currently a stub. The normalisation pipeline is implemented as a library (`pocketsmith_sync::normalise`) and exercised through tests.

### Categorise

Maps normalised payees to PocketSmith categories using a multi-source pipeline:

1. **Rule-based** -- type rules (salary -> `_Income`, transfer -> `_Transfer`) and payee overrides from `rules/categorise.yaml`
2. **Google Places API** -- looks up physical merchants to get place types (e.g. supermarket -> `_Groceries`)
3. **LLM fallback** -- Claude Haiku categorises unresolved merchants
4. **Cache** -- Google Places results are cached in SQLite to avoid repeat API calls

```
cargo run --bin categorise           # show proposed changes, prompt for approval
cargo run --bin categorise -- --yes  # auto-approve and apply
```

Target categories (12): `_Bills`, `_Dining`, `_Education`, `_Giving`, `_Groceries`, `_Holidays`, `_Household`, `_Income`, `_Mortgage`, `_Shopping`, `_Transfer`, `_Transport`

#### Configuration

Add to `.env`:

```
GOOGLE_PLACES_API_KEY=your_key_here   # optional, for merchant lookup
ANTHROPIC_API_KEY=your_key_here       # optional, for LLM fallback
```

Without API keys, only rule-based and cached categorisations run.

#### Iterative refinement

Use the `/categorise` command with `/loop` for continuous improvement:

```
/loop 5m /categorise
```

Each iteration runs categorisation, samples low-frequency payees for review, and applies corrections to rules.

## Project structure

```
src/
  main.rs              Default binary (sync)
  bin/normalise.rs     Normalise binary entry point
  bin/categorise.rs    Categorise binary entry point
  lib.rs               Library root
  client.rs            PocketSmith API client (ureq)
  sync.rs              Pull logic (API -> SQLite)
  models.rs            API request/response types
  db/                  SQLite layer (schema, upserts, change log)
  normalise/           5-stage payee normalisation pipeline
    strip.rs           Stage 1: prefix/suffix removal
    classify.rs        Stage 2: transaction type tagging
    expand.rs          Stage 3: truncation expansion
    identify.rs        Stage 4: entity matching
    cleanup.rs         Stage 5: final formatting
    rules.rs           YAML rule loading and compilation
    main.rs            Pipeline orchestrator
  categorise/          Transaction categorisation pipeline
    mod.rs             Shared types, confidence constants
    rules.rs           Rule-based categorisation (type + payee overrides)
    cache.rs           SQLite places_cache table
    places.rs          Google Places API client
    llm.rs             Claude Haiku LLM fallback
    mapping.rs         Google Places types -> PocketSmith categories
    pipeline.rs        Orchestrator
    eval.rs            Coverage reporting
rules/                 YAML rule definitions per stage
tests/                 Integration tests (live API)
```

## Tests

### Unit tests

```
cargo test
```

Covers model deserialisation, the normalisation pipeline (end-to-end and per-stage), and rule loading. These run offline with no API key required.

### Integration tests

Integration tests hit the live PocketSmith API and are marked `#[ignore]` by default. To run them:

```
POCKETSMITH_API_KEY=your_key cargo test -- --ignored
```

These tests cover the full API surface: fetching users, accounts, categories, transactions, and a complete transaction lifecycle (create, update, verify, delete).

## License

MIT
