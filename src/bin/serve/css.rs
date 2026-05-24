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
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    transition: background 0.1s;
}
.queue-item:hover { background: var(--bg-highlight); }
.queue-item.selected { background: var(--bg-highlight); border-left: 3px solid var(--accent); padding-left: 9px; }

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
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    background: var(--bg-highlight);
    color: var(--accent);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
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
"#;
