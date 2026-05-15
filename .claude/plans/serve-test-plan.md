# Test Plan: serve binary

## Unit tests (no server needed)

Pure functions that can be tested directly:

- **`format_dollars`** — positive/negative/zero/large amounts, comma formatting
- **`format_short_date`** — valid dates, invalid input
- **`parse_pair_id`** — valid IDs, malformed paths, missing segments
- **`extract_param`** — single/multiple params, missing key, empty value
- **`confidence_class` / `confidence_reason`** — all enum variants
- **`date_diff_days`** — same day, multi-day gap, reversed dates
- **`get_filtered_pairs`** — known DB data, verify each filter combination (all/pending/confirmed/rejected/skipped × conf levels)

## Integration tests (spin up server, use reqwest)

- `GET /` returns full HTML with expected structure
- `GET /queue?filter=X&conf=Y` returns correct subset
- `POST /pair/{id}/confirm` → pair moves to confirmed, stays visible in ALL view
- `POST /pair/{id}/reject` → pair moves to rejected, stays visible in ALL view
- `POST /pair/{id}/skip` → pair gains skip indicator, hidden from PENDING, visible in SKIPPED
- `POST /pair/{id}/unskip` → skip cleared, pair reappears in PENDING
- `POST /clear-all-skipped` → all skips cleared
- `POST /pair/{id}/undo` → reverts confirm/reject/skip, clears decision styling

## Keyboard shortcuts (verify JS emitted)

- Y/N/S keys trigger confirm/reject/skip
- U key triggers undo
- Arrow keys navigate queue

## Property-based / fuzz (optional)

- Random sequences of confirm/reject/skip/undo never panic
- `decisions` map stays consistent with `skipped_pairs` and DB status
