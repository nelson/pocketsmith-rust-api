pub const CSS: &str = r#"
:root {
    --bg: #1a1b26;
    --bg-surface: #24283b;
    --bg-highlight: #292e42;
    --border: #3b4261;
    --fg: #c0caf5;
    --fg-dim: #565f89;
    --fg-dark: #414868;
    --accent: #7aa2f7;
    --green: #9ece6a;
    --red: #f7768e;
    --yellow: #e0af68;
    --magenta: #bb9af7;
    --cyan: #7dcfff;
}

* { box-sizing: border-box; margin: 0; padding: 0; }

/* Header row: tab bar on the left, right-aligned sync chip + hints
 * trigger on the right. */
.header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 0 0.75rem 0;
}
.header .tab-bar { margin: 0; }
.header-right { margin-left: auto; display: flex; align-items: center; gap: 8px; }

.sync-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
    color: var(--fg-dim);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    cursor: help;
}
.sync-chip-dot {
    display: inline-block;
    width: 7px; height: 7px;
    border-radius: 50%;
    background: var(--fg-dim);
}
.sync-chip-fresh .sync-chip-dot { background: var(--green); }
.sync-chip-stale .sync-chip-dot { background: var(--yellow); }
.sync-chip-old   .sync-chip-dot { background: var(--red); }
.sync-chip-never .sync-chip-dot { background: var(--red); }

.hints-trigger {
    background: var(--bg-surface);
    color: var(--fg-dim);
    border: 1px solid var(--border);
    width: 26px; height: 26px;
    border-radius: 50%;
    cursor: pointer;
    font-size: 13px;
    font-weight: 700;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
}
.hints-trigger:hover { color: var(--fg); border-color: var(--accent); }

/* Keyboard-hints modal overlay (toggled by `?` or the header trigger). */
.hints-overlay {
    display: none;
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 100;
    align-items: center;
    justify-content: center;
}
.hints-overlay.open { display: flex; }
.hints-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 18px 22px;
    min-width: 420px;
    max-width: 560px;
    box-shadow: 0 12px 32px rgba(0,0,0,0.45);
}
.hints-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
}
.hints-card h2 { font-size: 14px; font-weight: 600; }
.hints-close {
    background: none;
    border: none;
    color: var(--fg-dim);
    font-size: 22px;
    cursor: pointer;
    line-height: 1;
}
.hints-close:hover { color: var(--fg); }
.hints-grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 16px;
    font-size: 12px;
}
.hints-grid .kbd {
    display: inline-block;
    background: var(--bg);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 4px;
    padding: 1px 8px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    color: var(--fg);
    text-align: center;
    min-width: 24px;
}
.hints-foot {
    margin-top: 14px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--fg-dim);
}

.tab-bar {
    display: flex;
    gap: 0.25rem;
    margin: 0 0 0.75rem 0;
    padding: 0.35rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    align-self: flex-start;
    width: fit-content;
}
.tab-bar .tab {
    display: inline-block;
    padding: 0.35rem 1rem;
    border-radius: 7px;
    color: var(--fg-dim);
    text-decoration: none;
    font-size: 0.9rem;
}
.tab-bar .tab:hover { color: var(--fg); background: var(--bg-highlight); }
.tab-bar .tab.active { background: var(--accent); color: var(--bg); }

body {
    background: var(--bg);
    color: var(--fg);
    font-family: "SF Hello", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 14px;
    line-height: 1.5;
    padding: 16px;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.layout {
    display: grid;
    grid-template-columns: 420px 1fr;
    gap: 16px;
    flex: 1;
    min-height: 0;
    margin-bottom: 16px;
}

@media (max-width: 768px) {
    body { overflow: auto; height: auto; }
    .layout {
        grid-template-columns: 1fr;
        flex: none;
    }
    .queue-panel { max-height: 35vh; }
}

/* Queue panel */
.queue-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
}

.queue-header {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
}

.queue-header h2 {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 8px;
    color: var(--fg);
}

.filter-row { display: flex; gap: 4px; margin-bottom: 4px; flex-wrap: wrap; }
.filter-row:last-child { margin-bottom: 0; }

.filter-btn {
    background: var(--bg-highlight);
    border: 1px solid var(--border);
    color: var(--fg-dim);
    padding: 2px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
.filter-btn:hover { border-color: var(--accent); color: var(--fg); }
.filter-btn.active { background: var(--accent); color: var(--bg); border-color: var(--accent); }

.queue-list { flex: 1; overflow-y: auto; min-height: 0; }

.queue-item {
    display: grid;
    grid-template-columns: 24px 1fr auto auto;
    gap: 8px;
    align-items: center;
    padding: 6px 12px;
    /* Reserve a 3px transparent left border on every row so toggling
       .selected (which fills the same border) does not shift the row
       content right. Without this, every selected row's columns slide
       3px to the right relative to the unselected rows. */
    border-left: 3px solid transparent;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    transition: background 0.1s;
}
.queue-item:hover { background: var(--bg-highlight); }
.queue-item.selected { background: var(--bg-highlight); border-left-color: var(--accent); }

.conf-badge {
    font-size: 10px;
    font-weight: 700;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
}
.conf-high .conf-badge { background: rgba(158, 206, 106, 0.15); color: var(--green); }
.conf-med .conf-badge { background: rgba(224, 175, 104, 0.15); color: var(--yellow); }
.conf-low .conf-badge { background: rgba(187, 154, 247, 0.15); color: var(--magenta); }

.queue-item .amount { color: var(--fg); text-align: right; }
.queue-item .date { color: var(--fg-dim); }
.queue-item .gap { color: var(--fg-dark); font-size: 11px; }

.queue-item.decided-confirmed { background: rgba(158, 206, 106, 0.08); }
.queue-item.decided-rejected { background: rgba(247, 118, 142, 0.08); }
.queue-item.decided-skipped { opacity: 0.5; }
.queue-item.decided-skipped .amount { text-decoration: line-through; }

.status-indicator {
    font-size: 12px;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    cursor: pointer;
}
.status-indicator:hover { transform: scale(1.2); }
.confirm-indicator { color: var(--green); background: rgba(158, 206, 106, 0.15); }
.reject-indicator { color: var(--red); background: rgba(247, 118, 142, 0.15); }
.skip-indicator { color: var(--fg-dim); background: var(--bg-highlight); }

.clear-skipped-btn { color: var(--yellow) !important; border-color: var(--yellow) !important; margin-left: auto; }

/* Bulk actions bar (sits under the filter rows in the queue header) */
.bulk-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    padding: 8px 0 0;
    border-top: 1px solid var(--border);
    margin-top: 8px;
}
.bulk-btn {
    background: var(--bg-surface);
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
}
.bulk-btn:disabled { opacity: 0.4; cursor: not-allowed; }
.bulk-btn:hover:not(:disabled) { border-color: var(--accent); }
.bulk-confirm-btn { color: var(--green); border-color: rgba(158, 206, 106, 0.4); }
.bulk-confirm-btn:hover:not(:disabled) { background: rgba(158, 206, 106, 0.15); border-color: var(--green); }
.bulk-reject-btn { color: var(--red); border-color: rgba(247, 118, 142, 0.4); }
.bulk-reject-btn:hover:not(:disabled) { background: rgba(247, 118, 142, 0.15); border-color: var(--red); }
.bulk-cancel-btn { color: var(--fg-dim); }
.bulk-yes { font-weight: 700; }
.bulk-prompt-text { color: var(--fg-dim); font-size: 11px; margin-right: 4px; }

/* Detail panel */
.detail-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 20px;
    overflow-y: auto;
    min-height: 0;
}

.detail-header h2 {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 4px;
}

.confidence-reason {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 16px;
    font-style: italic;
}

.status-badge { margin-left: 8px; }

/* Comparison layout */
.comparison { margin-bottom: 16px; }

.comparison-meta {
    display: flex;
    gap: 24px;
    margin-bottom: 12px;
    padding: 8px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}

.meta-item { display: flex; align-items: center; gap: 8px; }
.meta-label {
    font-size: 11px;
    color: var(--fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}
.meta-value {
    font-size: 14px;
    font-weight: 600;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}

.txn-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
}

@media (max-width: 768px) {
    .txn-cards { grid-template-columns: 1fr; gap: 8px; }
}

.txn-card {
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 12px;
    min-width: 0;
    overflow-wrap: anywhere;
    word-break: break-word;
}
.txn-card:first-child { border-radius: 6px 0 0 6px; }
.txn-card:last-child { border-radius: 0 6px 6px 0; }

.txn-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
}

.card-label {
    font-size: 11px;
    font-weight: 700;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    background: var(--bg-highlight);
    color: var(--accent);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    white-space: nowrap;
}

.card-account {
    font-size: 13px;
    font-weight: 600;
    color: var(--fg);
}

.txn-card-body { display: flex; flex-direction: column; gap: 4px; }

.field {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    min-width: 0;
    gap: 8px;
}
.field-label { color: var(--fg-dim); flex-shrink: 0; }
.field-value { color: var(--fg); text-align: right; min-width: 0; flex: 1; overflow-wrap: anywhere; word-break: break-word; }
.amount-positive { color: var(--green) !important; font-weight: 600; }
.amount-negative { color: var(--red) !important; font-weight: 600; }

/* Prior pairs */
.prior-section {
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}
.prior-section h3 {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 6px;
}
.prior-row {
    display: flex;
    gap: 16px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    padding: 2px 0;
}
.status-confirmed { color: var(--green); }
.status-rejected { color: var(--red); }
.status-skipped { color: var(--fg-dim); }

/* Normalise tab: matching-transactions list + extracted-features block */
.norm-txn-list {
    max-height: 320px;
    overflow-y: auto;
}
.norm-txn-list .prior-row {
    border-bottom: 1px solid var(--border);
    padding: 4px 0;
}
.norm-txn-list .prior-row:last-child { border-bottom: none; }
.norm-txn-list .prior-row > span:nth-child(2) {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.norm-txn-acct { color: var(--fg-dark); font-size: 11px; }

.norm-features {
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}
.norm-features h3 {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
.norm-features-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 16px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
}
.norm-feature-key { color: var(--fg-dim); }
.norm-feature-val { color: var(--fg); overflow-wrap: anywhere; }

/* Pipeline trace: per-stage before/after with feature attribution */
.norm-trace {
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}
.norm-trace h3 {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 8px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
.norm-trace-empty {
    color: var(--fg-dark);
    font-style: italic;
    font-size: 12px;
}
.norm-trace-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
}
.norm-trace-row {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 12px;
    align-items: start;
    padding: 4px 0;
    border-bottom: 1px dashed var(--border);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
}
.norm-trace-row:last-child { border-bottom: none; }
.norm-trace-stage {
    color: var(--accent);
    font-weight: 600;
}
.norm-trace-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.norm-trace-diff { overflow-wrap: anywhere; word-break: break-word; }
.norm-trace-before { color: var(--red); text-decoration: line-through; }
.norm-trace-arrow { color: var(--fg-dim); }
.norm-trace-after { color: var(--green); font-weight: 600; }
.norm-trace-extracted { display: flex; flex-wrap: wrap; gap: 8px; font-size: 11px; }
.norm-trace-class { color: var(--magenta); }
.norm-trace-feat { color: var(--cyan); }
.norm-trace-feat-val { color: var(--fg); font-weight: 600; }
.norm-trace-pattern {
    margin-top: 4px;
    font-size: 11px;
    color: var(--fg-dim);
}
.norm-trace-pattern-label {
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    color: var(--fg-dark);
    margin-right: 2px;
}
.norm-trace-pattern-src {
    background: var(--bg-highlight);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 5px;
    color: var(--magenta);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 11px;
    overflow-wrap: anywhere;
    word-break: break-all;
}

/* Actions */
.actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 16px;
}

.btn {
    padding: 8px 20px;
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 13px;
    font-weight: 600;
    transition: all 0.15s;
}
.btn:hover { transform: translateY(-1px); }

.btn-confirm { background: rgba(158, 206, 106, 0.15); color: var(--green); border-color: var(--green); }
.btn-confirm:hover { background: rgba(158, 206, 106, 0.25); }

.btn-reject { background: rgba(247, 118, 142, 0.15); color: var(--red); border-color: var(--red); }
.btn-reject:hover { background: rgba(247, 118, 142, 0.25); }

.btn-skip { background: var(--bg-highlight); color: var(--fg-dim); }
.btn-skip:hover { color: var(--fg); }

/* Activity panel */
.activity-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
    max-height: 160px;
    overflow-y: auto;
    flex-shrink: 0;
}

.activity-header {
    display: flex;
    gap: 20px;
    margin-bottom: 8px;
    font-size: 13px;
    flex-wrap: wrap;
}
.stat { color: var(--fg-dim); }
.count-confirmed { color: var(--green); font-weight: 600; }
.count-rejected { color: var(--red); font-weight: 600; }
.count-skipped { color: var(--fg-dim); font-weight: 600; }
.count-undone { color: var(--yellow); font-weight: 600; }
.count-applied { color: var(--accent); font-weight: 600; }

.apply-btn {
    margin-left: auto;
    background: rgba(122, 162, 247, 0.15);
    color: var(--accent);
    border: 1px solid var(--accent);
    padding: 4px 12px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-radius: 4px;
    cursor: pointer;
}
.apply-btn:hover:not(:disabled) { background: rgba(122, 162, 247, 0.3); }
.apply-btn:disabled { opacity: 0.3; cursor: not-allowed; }

.activity-row {
    display: flex;
    gap: 12px;
    align-items: center;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
}
.activity-row:last-child { border-bottom: none; }

.undo-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--fg-dim);
    padding: 1px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    margin-left: auto;
}
.undo-btn:hover { color: var(--yellow); border-color: var(--yellow); }

.empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: var(--fg-dim);
    font-size: 16px;
}
"/* ===========================================================
 * Per-pillar status emojis (Pair / Norm / Cat) used by the
 * Transactions tab's queue rows. Shape-distinct glyphs encode
 * dimension and state without relying on colour, so the
 * vocabulary is readable regardless of colour vision.
 * Mnemonic shape families: Pair = links, Norm = labels, Cat = filing.
 * =========================================================== */
.g-pair-confirmed::before { content: "\1F517"; }            /* chain link */
.g-pair-pending::before   { content: "\1F4CE"; }            /* paperclip */
.g-pair-orphan::before    { content: "\26D3 \FE0F \200D \1F4A5"; } /* broken chain (chains + ZWJ + collision) */
.g-pair-rejected::before  { content: "\2702\FE0F"; }        /* scissors */
.g-norm-confirmed::before { content: "\2705"; }       /* white heavy check mark */
.g-norm-pending::before   { content: "\1F50D"; }      /* magnifying glass */
.g-norm-missing::before   { content: "\2753"; }       /* red question mark */
.g-norm-rejected::before  { content: "\1F6AB"; }      /* prohibited */
.g-cat-confirmed::before  { content: "\1F5C4\FE0F"; } /* file cabinet */
.g-cat-pending::before    { content: "\1F4C1"; }      /* folder */
.g-cat-missing::before    { content: "\1F4E6"; }      /* package */
.g-cat-rejected::before   { content: "\1F5D1\FE0F"; } /* wastebasket */
.g-none::before           { content: "\B7"; opacity: 0.4; }
.g-pair-confirmed, .g-pair-pending, .g-pair-rejected, .g-pair-orphan,
.g-norm-confirmed, .g-norm-pending, .g-norm-rejected, .g-norm-missing,
.g-cat-confirmed, .g-cat-pending, .g-cat-rejected, .g-cat-missing,
.g-none {
    display: inline-block;
    min-width: 1.4em;
    text-align: center;
    font-style: normal;
    font-weight: normal;
}

/* ===========================================================
 * Transactions tab — queue row layout.
 *
 * The Transactions row shape is different from the Transfers /
 * Normalise rows (date column, three-emoji glyph stack, payee,
 * signed amount), so we override the shared .queue-item grid
 * just for this tab. Body element carries class="tab-transactions"
 * (set by render::render_page) so the override is scoped.
 * =========================================================== */
.tab-transactions .queue-item {
    /* Round-4 layout: date | norm-glyph | payee | post-payee group | amount
       Date is leftmost. The post-payee group bundles pair-glyph (when
       present) and cat-tag (when categorised) into a single grid cell
       so we don't pay two grid gaps when one or both are absent. */
    grid-template-columns: 56px 24px minmax(0, 1fr) auto auto;
    align-items: center;
    padding: 6px 12px;
}
.tab-transactions .queue-item .date {
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
    font-size: 11px;
}
.tab-transactions .queue-item .payee {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg);
}
.tab-transactions .queue-item .amount {
    font-variant-numeric: tabular-nums;
    text-align: right;
}
.tab-transactions .queue-item .amount-positive { color: var(--green); }
.tab-transactions .queue-item .amount-negative { color: var(--fg); }
.tab-transactions .queue-item .post-payee {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    justify-content: flex-end;
}

/* Category tag (small squared box) used in queue rows and detail header. */
.tab-transactions .cat-tag {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 10px;
    border: 1px solid var(--border);
    color: var(--fg-dim);
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: help;
    line-height: 1.4;
}
.tab-transactions .cat-tag-confirmed { color: var(--fg-dim); border-color: var(--fg-dark); }
.tab-transactions .cat-tag-pending   { color: var(--yellow); border-color: var(--yellow); }
.tab-transactions .cat-tag-rejected  { color: var(--fg-dim); border-color: var(--fg-dim); }
.tab-transactions .clickable { cursor: pointer; }
.tab-transactions .clickable:hover { transform: scale(1.15); transition: transform 0.1s; }

/* ===========================================================
 * Dashboard tab.
 * =========================================================== */
.dash-queue-help { font-size: 11px; color: var(--fg-dim); margin-top: 4px; line-height: 1.3; }

.tab-dashboard .queue-item.month-row {
    grid-template-columns: 64px minmax(0, 1fr) auto;
    align-items: center;
    padding: 8px 12px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
}
.tab-dashboard .month-label { color: var(--fg); font-weight: 600; font-variant-numeric: tabular-nums; }
.tab-dashboard .month-figs { color: var(--fg-dim); font-size: 11px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tab-dashboard .month-figs .amount-positive,
.tab-dashboard .month-figs .amount-negative { font-weight: 500; }
.tab-dashboard .net-pos { color: var(--green); font-weight: 600; }
.tab-dashboard .net-neg { color: var(--red);   font-weight: 600; }

.tab-dashboard .hyg-dots { display: inline-flex; gap: 3px; align-items: center; }
.tab-dashboard .hyg-dot {
    display: inline-block;
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--fg-dark);
}
.tab-dashboard .hyg-on   { background: var(--green); }
.tab-dashboard .hyg-warn { background: var(--yellow); }
.tab-dashboard .hyg-bad  { background: var(--red); }

.tab-dashboard .detail-header { padding-bottom: 10px; border-bottom: 1px solid var(--border); margin-bottom: 12px; }
.tab-dashboard .detail-header .row { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; }
.tab-dashboard .detail-header h2 { font-size: 18px; font-weight: 500; }
.tab-dashboard .detail-header .amount-big { font-size: 18px; font-variant-numeric: tabular-nums; }
.tab-dashboard .detail-header .meta { color: var(--fg-dim); font-size: 12px; margin-top: 6px; display: flex; gap: 8px; flex-wrap: wrap; }
.tab-dashboard .chip { display: inline-block; padding: 1px 8px; border: 1px solid var(--border); border-radius: 999px; font-size: 11px; color: var(--fg-dim); }

.tab-dashboard .dash-month-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
}
@media (max-width: 1100px) {
    .tab-dashboard .dash-month-grid { grid-template-columns: 1fr; }
}
.tab-dashboard .dash-sankey-wrap,
.tab-dashboard .dash-breakdown-wrap {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 12px 14px;
}
.tab-dashboard .dash-sankey-wrap h3,
.tab-dashboard .dash-breakdown-wrap h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-dim);
    font-weight: 500;
    margin-bottom: 4px;
}
.tab-dashboard .dash-sankey-wrap .sub,
.tab-dashboard .dash-breakdown-wrap .sub { color: var(--fg-dim); font-size: 11px; margin-bottom: 8px; }
.tab-dashboard .dash-sankey { width: 100%; height: auto; display: block; }

.tab-dashboard .dash-breakdown {
    width: 100%;
    border-collapse: collapse;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
}
.tab-dashboard .dash-breakdown th,
.tab-dashboard .dash-breakdown td {
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
    text-align: left;
}
.tab-dashboard .dash-breakdown th {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
    font-weight: 500;
}
.tab-dashboard .dash-breakdown .align-right { text-align: right; }
.tab-dashboard .dash-breakdown .align-left { text-align: left; }
.tab-dashboard .dash-section-row td {
    background: var(--bg-highlight);
    color: var(--fg-dim);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 6px;
}
.tab-dashboard .empty-state-row { color: var(--fg-dim); font-style: italic; padding: 12px; }

.tab-dashboard .kbd-inline {
    display: inline-block;
    background: var(--bg);
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 3px;
    padding: 0 5px;
    margin: 0 2px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    color: var(--fg);
    font-size: 11px;
}

/* Detail panel — cleaning state cards (one per pillar that needs
 * the user's attention). Bordered cards, dimension-coloured left edge,
 * inline glyph + title + sub. */
.tab-transactions .detail-header { padding-bottom: 10px; border-bottom: 1px solid var(--border); margin-bottom: 12px; }
.tab-transactions .detail-header .row { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; }
.tab-transactions .detail-header h2 { font-size: 16px; font-weight: 500; }
.tab-transactions .detail-header .amount-big { font-size: 18px; font-variant-numeric: tabular-nums; }
.tab-transactions .detail-header .amount-big.amount-positive { color: var(--green); }
.tab-transactions .detail-header .amount-big.amount-negative { color: var(--fg); }
.tab-transactions .detail-header .meta { color: var(--fg-dim); font-size: 12px; margin-top: 6px; display: flex; gap: 12px; flex-wrap: wrap; }
.tab-transactions .chip {
    display: inline-block; padding: 1px 8px;
    border: 1px solid var(--border); border-radius: 999px;
    font-size: 11px; color: var(--fg-dim);
}
/* Cleaning-state cards (Pair / Normalise / Categorise). Three cards
 * laid out horizontally so the entire pillar state is visible at a
 * glance. Visual language matches the transfers tab's .txn-card:
 * dark-box background, no coloured left border. The pillar status is
 * communicated by a chip in the card header (ok / pending / needs you).
 * Action buttons render inside the card when there's a decision
 * pending, so the user's eye doesn't have to leave the card. */
.tab-transactions .cleaning-cards {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin: 12px 0;
}
@media (max-width: 768px) {
    .tab-transactions .cleaning-cards { grid-template-columns: 1fr; }
}
.tab-transactions .cleaning-card {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
}
.tab-transactions .cleaning-card-na { opacity: 0.45; }
.tab-transactions .cleaning-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
}
.tab-transactions .cleaning-card-pillar {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-dim);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}
.tab-transactions .cleaning-card-status {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    color: var(--fg-dim);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}
.tab-transactions .card-status-ok   { color: var(--green);  border-color: rgba(158, 206, 106, 0.4); background: rgba(158, 206, 106, 0.08); }
.tab-transactions .card-status-warn { color: var(--yellow); border-color: rgba(224, 175, 104, 0.4); background: rgba(224, 175, 104, 0.08); }
.tab-transactions .card-status-bad  { color: var(--red);    border-color: rgba(247, 118, 142, 0.4); background: rgba(247, 118, 142, 0.08); }
.tab-transactions .cleaning-card-body {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 10px;
    align-items: start;
    flex: 1;
}
.tab-transactions .cleaning-card .glyph { font-size: 20px; line-height: 1; }
.tab-transactions .cleaning-card-text { min-width: 0; }
.tab-transactions .cleaning-card .title { font-size: 12px; color: var(--fg); margin-bottom: 2px; line-height: 1.3; }
.tab-transactions .cleaning-card .sub   { font-size: 11px; color: var(--fg-dim); line-height: 1.35; }
.tab-transactions .cleaning-card .actions { margin: 0; padding-top: 6px; border-top: 1px solid var(--border); }
.tab-transactions .cleaning-card .actions .btn { padding: 4px 10px; font-size: 11px; }
.tab-transactions .note {
    background: var(--bg-highlight); border-left: 3px solid var(--accent);
    padding: 8px 12px; font-size: 11px; color: var(--fg-dim);
    margin: 12px 0; border-radius: 0 6px 6px 0;
}
"#;
