# Refactor: Split serve.rs into smaller files

Convert `src/bin/serve.rs` into a multi-file binary at `src/bin/serve/`:

```
src/bin/serve/
├── main.rs          — entry point, server setup, request routing
├── state.rs         — AppState struct, ActivityEntry
├── handlers.rs      — action_handler, undo_handler, unskip_handler, clear_all_skipped
├── views.rs         — full_page, page_shell, render_queue, render_detail, render_activity
├── helpers.rs       — format_dollars, confidence_class, confidence_reason, date_diff_days,
│                      format_short_date, get_prior_pairs, get_filtered_pairs, extract_param, parse_pair_id
├── css.rs           — CSS const
└── js.rs            — JS const
```

## Rationale

The file is ~1000 lines and growing. The natural seams are: routing/dispatch, state, request handlers (mutate state), view rendering (read state), utility helpers, and static assets.

## Migration approach

Move one module at a time, bottom-up:

1. `css.rs` and `js.rs` — static consts, zero deps
2. `helpers.rs` — pure functions, depends only on external crates and library types
3. `state.rs` — struct definitions, depends on library types
4. `views.rs` — rendering functions, depends on state + helpers
5. `handlers.rs` — mutation logic, depends on state + views + helpers
6. `main.rs` — routing, depends on all modules

Each step should compile and the binary should behave identically before proceeding to the next.
